//! Analysis calculations (#181): writing to disk and conversion.
//!
//! The operation goes through the file in the form of a generic tree
//! (`stokesforge_analysis::tree::Tree`, converted to `rmpv::Value`): a new
//! setting therefore does not change the schema. Tolerant reading (invariant 10):
//! a calculation whose operation is unreadable is dropped — its columns stay
//! ordinary columns —, an unknown result value or warning
//! is ignored.

use std::collections::BTreeMap;
use std::sync::Arc;

use rmpv::Value;
use serde::{Deserialize, Serialize};
use stokesforge_analysis::ops::{Operation, Warning};
use stokesforge_analysis::tree::Tree;
use stokesforge_core::{
    AuxRef, Calculation, CorrectionRecord, NodeId, OutputRef, ResultValue, SeriesRef, SeriesResult,
    Staleness, Tree as ModelTree,
};

use crate::DiscardedCalculation;

/// Default value of an active boolean when the field is missing from the file.
fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct CalculationRecord {
    pub(crate) id: u64,
    pub(crate) operation: Value,
    #[serde(default)]
    pub(crate) series: Vec<SeriesRefRecord>,
    #[serde(default)]
    pub(crate) aux: Option<AuxRefRecord>,
    #[serde(default)]
    pub(crate) outputs: Vec<OutputRefRecord>,
    #[serde(default)]
    pub(crate) results: Vec<SeriesResultRecord>,
    #[serde(default)]
    pub(crate) keep_results: bool,
    #[serde(default)]
    pub(crate) auto_recalc: bool,
    /// Dialog when R² is low (#288); active by default.
    #[serde(default = "default_true")]
    pub(crate) low_r2_dialog: bool,
    /// `"manual"` or `"input_missing"`; a recalculation failure is written
    /// `"manual"` (the next recalculation finds the error again).
    #[serde(default)]
    pub(crate) stale: Option<String>,
    /// Fingerprint of the content of the input columns at save
    /// (A1, #316: `Calculation::inputs_digest`). Absent from an older
    /// file, or when an input was missing: the calculation is then taken as is.
    #[serde(default)]
    pub(crate) inputs_digest: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct SeriesRefRecord {
    #[serde(default)]
    pub(crate) x: Option<u64>,
    pub(crate) y: u64,
    #[serde(default)]
    pub(crate) sigma: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct AuxRefRecord {
    #[serde(default)]
    pub(crate) x: Option<u64>,
    pub(crate) y: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct OutputRefRecord {
    pub(crate) series: u64,
    pub(crate) key: String,
    #[serde(default)]
    pub(crate) index: Option<u64>,
    #[serde(default)]
    pub(crate) column: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub(crate) struct SeriesResultRecord {
    #[serde(default)]
    pub(crate) values: Vec<ResultValueRecord>,
    #[serde(default)]
    pub(crate) skipped: u64,
    #[serde(default)]
    pub(crate) warnings: Vec<WarningRecord>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct ResultValueRecord {
    pub(crate) key: String,
    #[serde(default)]
    pub(crate) index: Option<u64>,
    pub(crate) value: f64,
    #[serde(default)]
    pub(crate) error: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct WarningRecord {
    pub(crate) kind: String,
    #[serde(default)]
    pub(crate) count: u64,
}

// ── Operation tree ↔ MessagePack value ──────────────────────────────────

fn tree_to_value(tree: &Tree) -> Value {
    match tree {
        Tree::Null => Value::Nil,
        Tree::Bool(b) => Value::Boolean(*b),
        Tree::Num(n) => Value::F64(*n),
        Tree::Text(t) => Value::String(t.as_str().into()),
        Tree::List(items) => Value::Array(items.iter().map(tree_to_value).collect()),
        Tree::Map(entries) => Value::Map(
            entries
                .iter()
                .map(|(k, v)| (Value::String(k.as_str().into()), tree_to_value(v)))
                .collect(),
        ),
    }
}

fn value_to_tree(value: &Value) -> Tree {
    match value {
        Value::Boolean(b) => Tree::Bool(*b),
        Value::Integer(i) => i.as_f64().map(Tree::Num).unwrap_or(Tree::Null),
        Value::F32(f) => Tree::Num(f64::from(*f)),
        Value::F64(f) => Tree::Num(*f),
        Value::String(s) => s
            .as_str()
            .map(|t| Tree::Text(t.to_string()))
            .unwrap_or(Tree::Null),
        Value::Array(items) => Tree::List(items.iter().map(value_to_tree).collect()),
        Value::Map(entries) => Tree::Map(
            entries
                .iter()
                .filter_map(|(k, v)| Some((k.as_str()?.to_string(), value_to_tree(v))))
                .collect(),
        ),
        Value::Nil | Value::Binary(_) | Value::Ext(..) => Tree::Null,
    }
}

// ── Model → record ──────────────────────────────────────────────────────

fn warning_to_record(w: &Warning) -> WarningRecord {
    match w {
        Warning::SignalStepNotUniform => WarningRecord {
            kind: "signal_step_not_uniform".into(),
            count: 0,
        },
        Warning::IrfStepMismatch => WarningRecord {
            kind: "irf_step_mismatch".into(),
            count: 0,
        },
        Warning::NegativeClipped(n) => WarningRecord {
            kind: "negative_clipped".into(),
            count: *n as u64,
        },
        Warning::NotConverged => WarningRecord {
            kind: "not_converged".into(),
            count: 0,
        },
        Warning::Undetermined(n) => WarningRecord {
            kind: "undetermined".into(),
            count: *n as u64,
        },
        Warning::IrfTruncated => WarningRecord {
            kind: "irf_truncated".into(),
            count: 0,
        },
        Warning::BridgedReferenceGap(n) => WarningRecord {
            kind: "bridged_reference_gap".into(),
            count: *n as u64,
        },
        Warning::DivisionByZero(n) => WarningRecord {
            kind: "division_by_zero".into(),
            count: *n as u64,
        },
        Warning::ShiftedX => WarningRecord {
            kind: "shifted_x".into(),
            count: 0,
        },
    }
}

fn record_to_warning(r: &WarningRecord) -> Option<Warning> {
    match r.kind.as_str() {
        "signal_step_not_uniform" => Some(Warning::SignalStepNotUniform),
        "irf_step_mismatch" => Some(Warning::IrfStepMismatch),
        "negative_clipped" => Some(Warning::NegativeClipped(r.count as usize)),
        "not_converged" => Some(Warning::NotConverged),
        "undetermined" => Some(Warning::Undetermined(r.count as usize)),
        "irf_truncated" => Some(Warning::IrfTruncated),
        "bridged_reference_gap" => Some(Warning::BridgedReferenceGap(r.count as usize)),
        "division_by_zero" => Some(Warning::DivisionByZero(r.count as usize)),
        "shifted_x" => Some(Warning::ShiftedX),
        _ => None,
    }
}

pub(crate) fn calculations_to_records(tree: &ModelTree) -> Vec<CalculationRecord> {
    map_to_records(tree.calculations(), Some(tree))
}

/// `digest_in`: the tree where to read the inputs to write their fingerprint
/// (`None` for the correction traces, which do not go stale).
fn map_to_records(
    calculations: &BTreeMap<NodeId, Arc<Calculation>>,
    digest_in: Option<&ModelTree>,
) -> Vec<CalculationRecord> {
    calculations
        .iter()
        .map(|(id, calc)| CalculationRecord {
            id: id.raw(),
            operation: tree_to_value(&calc.operation.to_tree()),
            series: calc
                .series
                .iter()
                .map(|s| SeriesRefRecord {
                    x: s.x.map(NodeId::raw),
                    y: s.y.raw(),
                    sigma: s.sigma.map(NodeId::raw),
                })
                .collect(),
            aux: calc.aux.as_ref().map(|a| AuxRefRecord {
                x: a.x.map(NodeId::raw),
                y: a.y.raw(),
            }),
            outputs: calc
                .outputs
                .iter()
                .map(|o| OutputRefRecord {
                    series: o.series as u64,
                    key: o.key.clone(),
                    index: o.index.map(|i| i as u64),
                    column: o.column.map(NodeId::raw),
                })
                .collect(),
            results: calc
                .results
                .iter()
                .map(|r| SeriesResultRecord {
                    values: r
                        .values
                        .iter()
                        .map(|v| ResultValueRecord {
                            key: v.key.clone(),
                            index: v.index.map(|i| i as u64),
                            value: v.value,
                            error: v.error,
                        })
                        .collect(),
                    skipped: r.skipped as u64,
                    warnings: r.warnings.iter().map(warning_to_record).collect(),
                })
                .collect(),
            keep_results: calc.keep_results,
            auto_recalc: calc.auto_recalc,
            low_r2_dialog: calc.low_r2_dialog,
            stale: calc.stale.as_ref().map(|s| {
                match s {
                    Staleness::InputMissing => "input_missing",
                    Staleness::Manual | Staleness::Failed(_) => "manual",
                }
                .to_string()
            }),
            inputs_digest: digest_in.and_then(|tree| calc.inputs_digest(tree)),
        })
        .collect()
}

/// Trace of a correction that replaced its source data (#212, #248).
/// A separate file field: a trace never becomes an active calculation at
/// read. It carries the operation, the series, the frozen reference and the
/// results like a calculation, but no outputs, no recalculation flags, no
/// fingerprint: a trace does not go stale.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct CorrectionTraceRecord {
    pub(crate) id: u64,
    pub(crate) operation: Value,
    pub(crate) series: Vec<SeriesRefRecord>,
    pub(crate) aux: Option<AuxRefRecord>,
    pub(crate) results: Vec<SeriesResultRecord>,
    /// The replaced columns and the copy of their original.
    pub(crate) originals: Vec<OriginalRecord>,
}

/// A column replaced by a correction and the copy of its original
/// (hidden workbook of the analyses), enough to restore it (#248).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct OriginalRecord {
    pub(crate) column: u64,
    pub(crate) copy: u64,
}

/// The correction traces, to their file field.
pub(crate) fn corrections_to_records(tree: &ModelTree) -> Vec<CorrectionTraceRecord> {
    let traces = tree
        .correction_traces()
        .iter()
        .map(|(id, trace)| {
            (
                *id,
                Arc::new(Calculation {
                    operation: trace.operation.clone(),
                    series: trace.series.clone(),
                    aux: trace.aux.clone(),
                    outputs: Vec::new(),
                    results: trace.results.clone(),
                    keep_results: true,
                    auto_recalc: false,
                    low_r2_dialog: true,
                    stale: None,
                }),
            )
        })
        .collect();
    map_to_records(&traces, None)
        .into_iter()
        .map(|r| CorrectionTraceRecord {
            originals: tree
                .correction_traces()
                .get(&NodeId::from_raw(r.id))
                .map(|trace| {
                    trace
                        .originals
                        .iter()
                        .map(|(column, copy)| OriginalRecord {
                            column: column.raw(),
                            copy: copy.raw(),
                        })
                        .collect()
                })
                .unwrap_or_default(),
            id: r.id,
            operation: r.operation,
            series: r.series,
            aux: r.aux,
            results: r.results,
        })
        .collect()
}

/// Returns the kept traces and the discarded ones: a trace that is unreadable (or
/// fails validation), or whose operation is not a correction,
/// is counted (#309) instead of disappearing silently.
pub(crate) fn records_to_corrections(
    records: Vec<CorrectionTraceRecord>,
    max_id: u64,
) -> (
    BTreeMap<NodeId, Arc<CorrectionRecord>>,
    Vec<DiscardedCalculation>,
) {
    // A trace whose original exceeds the identifier bound is dropped
    // entirely, like a calculation one of whose columns exceeds it.
    let (within, beyond): (Vec<_>, Vec<_>) = records.into_iter().partition(|r| {
        r.originals
            .iter()
            .all(|o| o.column <= max_id && o.copy <= max_id)
    });
    let originals: BTreeMap<u64, Vec<OriginalRecord>> = {
        let mut seen = BTreeMap::new();
        for r in &within {
            // Two traces with the same id: the first prevails (#309, F4).
            seen.entry(r.id).or_insert_with(|| r.originals.clone());
        }
        seen
    };
    // Two records with the same id: the first prevails (#309, F4).
    let mut named: BTreeMap<u64, DiscardedCalculation> = BTreeMap::new();
    let as_calculation = |r: &CorrectionTraceRecord| CalculationRecord {
        id: r.id,
        operation: r.operation.clone(),
        series: r.series.clone(),
        aux: r.aux.clone(),
        outputs: Vec::new(),
        results: r.results.clone(),
        keep_results: true,
        auto_recalc: false,
        low_r2_dialog: true,
        stale: None,
        inputs_digest: None,
    };
    let calculations: Vec<CalculationRecord> = within.iter().map(as_calculation).collect();
    for r in &calculations {
        named
            .entry(r.id)
            .or_insert_with(|| discarded_calculation(r));
    }
    let (kept, mut discarded) = records_to_calculations(calculations, max_id);
    discarded.extend(
        beyond
            .iter()
            .map(|r| discarded_calculation(&as_calculation(r))),
    );
    let mut out = BTreeMap::new();
    for (id, c) in kept {
        if !matches!(
            c.operation,
            Operation::Baseline { .. } | Operation::Correct { .. } | Operation::Smooth { .. }
        ) {
            if let Some(d) = named.get(&id.raw()) {
                discarded.push(d.clone());
            }
            continue;
        }
        out.insert(
            id,
            Arc::new(CorrectionRecord {
                operation: c.operation.clone(),
                series: c.series.clone(),
                aux: c.aux.clone(),
                results: c.results.clone(),
                originals: originals
                    .get(&id.raw())
                    .map(|list| {
                        list.iter()
                            .map(|o| (NodeId::from_raw(o.column), NodeId::from_raw(o.copy)))
                            .collect()
                    })
                    .unwrap_or_default(),
            }),
        );
    }
    (out, discarded)
}

// ── Record → model ──────────────────────────────────────────────────────

/// An identifier beyond `max` drops the whole calculation (same bound as
/// the nodes, B1); so does an unreadable operation, or one over the cap (`validate`).
/// Returns the kept calculations and the list of those that are dropped (#278): an unreadable
/// record is not kept, but it is no longer lost silently.
pub(crate) fn records_to_calculations(
    records: Vec<CalculationRecord>,
    max_id: u64,
) -> (
    BTreeMap<NodeId, Arc<Calculation>>,
    Vec<DiscardedCalculation>,
) {
    let (out, discarded, _digests) = records_to_calculations_with_digests(records, max_id);
    (out, discarded)
}

/// Kept calculations, discarded calculations, input fingerprints written in the file.
type ConvertedCalculations = (
    BTreeMap<NodeId, Arc<Calculation>>,
    Vec<DiscardedCalculation>,
    BTreeMap<NodeId, u64>,
);

/// Like [`records_to_calculations`], with in addition the input fingerprint
/// written in the file for each **kept** calculation that carried one.
pub(crate) fn records_to_calculations_with_digests(
    records: Vec<CalculationRecord>,
    max_id: u64,
) -> ConvertedCalculations {
    let mut out = BTreeMap::new();
    let mut digests = BTreeMap::new();
    let mut discarded = Vec::new();
    for r in records {
        let mut all_ids = std::iter::once(r.id)
            .chain(
                r.series
                    .iter()
                    .flat_map(|s| std::iter::once(s.y).chain(s.x).chain(s.sigma)),
            )
            .chain(r.aux.iter().flat_map(|a| std::iter::once(a.y).chain(a.x)))
            .chain(r.outputs.iter().filter_map(|o| o.column));
        if !all_ids.all(|i| i <= max_id) {
            discarded.push(discarded_calculation(&r));
            continue;
        }
        // `from_tree` only judges the shape: a `NaN` read as is slips through.
        // `validate` is the same filter as that of `add_calculation` (core):
        // a calculation that fails it could never have been saved by
        // the application, it is a file modified by hand (#291).
        let Some(operation) = Operation::from_tree(&value_to_tree(&r.operation))
            .ok()
            .filter(|op| op.validate().is_ok())
        else {
            discarded.push(discarded_calculation(&r));
            continue;
        };
        let id = NodeId::from_raw(r.id);
        let node = |i: u64| NodeId::from_raw(i);
        let calc = Calculation {
            operation,
            series: r
                .series
                .iter()
                .map(|s| SeriesRef {
                    x: s.x.map(node),
                    y: node(s.y),
                    sigma: s.sigma.map(node),
                })
                .collect(),
            aux: r.aux.as_ref().map(|a| AuxRef {
                x: a.x.map(node),
                y: node(a.y),
            }),
            outputs: r
                .outputs
                .iter()
                .map(|o| OutputRef {
                    series: o.series as usize,
                    key: o.key.clone(),
                    index: o.index.map(|i| i as usize),
                    column: o.column.map(node),
                })
                .collect(),
            results: r
                .results
                .iter()
                .map(|s| SeriesResult {
                    values: s
                        .values
                        .iter()
                        .map(|v| ResultValue {
                            key: v.key.clone(),
                            index: v.index.map(|i| i as usize),
                            value: v.value,
                            error: v.error,
                        })
                        .collect(),
                    skipped: s.skipped as usize,
                    warnings: s.warnings.iter().filter_map(record_to_warning).collect(),
                })
                .collect(),
            keep_results: r.keep_results,
            auto_recalc: r.auto_recalc,
            low_r2_dialog: r.low_r2_dialog,
            stale: match r.stale.as_deref() {
                Some("input_missing") => Some(Staleness::InputMissing),
                Some(_) => Some(Staleness::Manual),
                None => None,
            },
        };
        // Same shape rule as at write (core, #309).
        if calc.validate_shape().is_err() {
            discarded.push(discarded_calculation(&r));
            continue;
        }
        // An identifier already kept: the first prevails, the second is
        // counted among the discarded instead of replacing it silently (#309).
        if out.contains_key(&id) {
            discarded.push(discarded_calculation(&r));
            continue;
        }
        if let Some(digest) = r.inputs_digest {
            digests.insert(id, digest);
        }
        out.insert(id, Arc::new(calc));
    }
    (out, discarded, digests)
}

/// A1 (#316): compares, **without recomputing anything**, the fingerprint written in the
/// file with the content of the inputs as the file carries it. A calculation whose
/// fingerprint differs is marked stale (`Manual`); its values stay
/// those of the file. A calculation without a fingerprint (old file, input
/// absent at write) or one of whose inputs is missing today is left as
/// it is: invariant 10 wants a degradation, never a failure.
pub(crate) fn mark_changed_inputs(
    calculations: &mut BTreeMap<NodeId, Arc<Calculation>>,
    digests: &BTreeMap<NodeId, u64>,
    tree: &ModelTree,
) {
    for (id, calc) in calculations.iter_mut() {
        let Some(&written) = digests.get(id) else {
            continue;
        };
        if calc.stale.is_some() {
            continue;
        }
        if calc
            .inputs_digest(tree)
            .is_some_and(|current| current != written)
        {
            Arc::make_mut(calc).stale = Some(Staleness::Manual);
        }
    }
}

/// What is known of a calculation that is dropped: its identifier, and the family
/// of operation when the record still carries it legibly.
fn discarded_calculation(r: &CalculationRecord) -> DiscardedCalculation {
    let kind = match value_to_tree(&r.operation) {
        Tree::Map(entries) => entries
            .into_iter()
            .find_map(|(k, v)| match (k.as_str(), v) {
                ("kind", Tree::Text(kind)) => Some(kind),
                _ => None,
            }),
        _ => None,
    };
    DiscardedCalculation { id: r.id, kind }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn trace(r: CalculationRecord) -> CorrectionTraceRecord {
        CorrectionTraceRecord {
            id: r.id,
            operation: r.operation,
            series: r.series,
            aux: r.aux,
            results: r.results,
            originals: Vec::new(),
        }
    }

    /// The originals of a trace go through the file, including that of a
    /// column that is the Y of no series; an original beyond the bound
    /// drops the whole trace.
    #[test]
    fn the_originals_of_a_trace_go_through_the_record() {
        let mut r = trace(record(good_operation(), 7));
        r.originals = vec![
            OriginalRecord {
                column: 7,
                copy: 90,
            },
            OriginalRecord {
                column: 40,
                copy: 91,
            },
        ];
        let (kept, discarded) = records_to_corrections(vec![r.clone()], 1000);
        assert!(discarded.is_empty());
        assert_eq!(
            kept[&NodeId::from_raw(50)].originals,
            vec![
                (NodeId::from_raw(7), NodeId::from_raw(90)),
                (NodeId::from_raw(40), NodeId::from_raw(91)),
            ]
        );
        r.originals[1].copy = 5000;
        let (kept, discarded) = records_to_corrections(vec![r], 1000);
        assert!(kept.is_empty());
        assert_eq!(discarded.len(), 1);
    }

    fn record(operation: Value, y: u64) -> CalculationRecord {
        CalculationRecord {
            id: 50,
            operation,
            series: vec![SeriesRefRecord {
                x: None,
                y,
                sigma: None,
            }],
            aux: None,
            outputs: vec![],
            results: vec![],
            keep_results: false,
            auto_recalc: true,
            low_r2_dialog: true,
            stale: None,
            inputs_digest: None,
        }
    }

    fn good_operation() -> Value {
        tree_to_value(
            &Operation::Correct {
                op: stokesforge_analysis::correct::CorrectOp::Divide,
            }
            .to_tree(),
        )
    }

    #[test]
    fn an_unreadable_operation_discards_the_calculation_without_failure() {
        let (kept, discarded) = records_to_calculations(vec![record(good_operation(), 7)], 1000);
        assert_eq!(kept.len(), 1);
        assert!(discarded.is_empty());
        let junk = Value::Map(vec![(Value::from("kind"), Value::from("teleportation"))]);
        let (kept, discarded) = records_to_calculations(vec![record(junk, 7)], 1000);
        assert!(kept.is_empty());
        // The discrepancy is reported, with the family when it can be read (#278).
        assert_eq!(
            discarded,
            vec![DiscardedCalculation {
                id: 50,
                kind: Some("teleportation".into())
            }]
        );
        let (kept, discarded) = records_to_calculations(vec![record(Value::Nil, 7)], 1000);
        assert!(kept.is_empty());
        assert_eq!(discarded, vec![DiscardedCalculation { id: 50, kind: None }]);
    }

    #[test]
    fn an_identifier_out_of_bounds_discards_the_calculation() {
        let (kept, discarded) = records_to_calculations(vec![record(good_operation(), 5000)], 1000);
        assert!(kept.is_empty());
        assert_eq!(
            discarded,
            vec![DiscardedCalculation {
                id: 50,
                kind: Some("correct".into())
            }],
            "the identifier out of bounds is reported, family read"
        );
        // Right on the bound: kept, nothing to report.
        let (kept, discarded) = records_to_calculations(vec![record(good_operation(), 1000)], 1000);
        assert_eq!(kept.len(), 1);
        assert!(discarded.is_empty());
        // Every identifier of the record is bounded: its own, those of an
        // output, of an x, of a sigma, of a reference; the bound does not
        // drop the sound neighbour.
        let mut own = record(good_operation(), 7);
        own.id = 1001;
        let mut output = record(good_operation(), 7);
        output.id = 51;
        output.outputs = vec![OutputRefRecord {
            series: 0,
            key: "corrected".into(),
            index: None,
            column: Some(1001),
        }];
        let mut healthy = record(good_operation(), 7);
        healthy.id = 52;
        let mut x = record(good_operation(), 7);
        x.id = 53;
        x.series[0].x = Some(1001);
        let mut sigma = record(good_operation(), 7);
        sigma.id = 54;
        sigma.series[0].sigma = Some(1001);
        let mut aux = record(good_operation(), 7);
        aux.id = 55;
        aux.aux = Some(AuxRefRecord { x: None, y: 1001 });
        let mut aux_x = record(good_operation(), 7);
        aux_x.id = 56;
        aux_x.aux = Some(AuxRefRecord {
            x: Some(1001),
            y: 7,
        });
        let (kept, discarded) =
            records_to_calculations(vec![own, output, healthy, x, sigma, aux, aux_x], 1000);
        assert_eq!(kept.keys().map(|k| k.raw()).collect::<Vec<_>>(), vec![52]);
        assert_eq!(
            discarded.iter().map(|d| d.id).collect::<Vec<_>>(),
            vec![1001, 51, 53, 54, 55, 56],
            "in the order of the records"
        );
    }

    #[test]
    fn several_discarded_calculations_are_all_listed_and_the_sound_ones_kept_in_order() {
        let junk = || Value::Map(vec![(Value::from("kind"), Value::from("teleportation"))]);
        let mut records = Vec::new();
        for (id, operation) in [
            (10, junk()),
            (11, good_operation()),
            (12, Value::Nil),
            (13, good_operation()),
            (14, junk()),
        ] {
            let mut r = record(operation, 7);
            r.id = id;
            records.push(r);
        }
        let (kept, discarded) = records_to_calculations(records, 1000);
        assert_eq!(
            kept.keys().map(|k| k.raw()).collect::<Vec<_>>(),
            vec![11, 13]
        );
        assert_eq!(
            discarded,
            vec![
                DiscardedCalculation {
                    id: 10,
                    kind: Some("teleportation".into())
                },
                DiscardedCalculation { id: 12, kind: None },
                DiscardedCalculation {
                    id: 14,
                    kind: Some("teleportation".into())
                },
            ]
        );
    }

    /// #309: the shape of a calculation is checked again at read (same rule as
    /// at write, core); the dropped one is named, never a panic.
    #[test]
    fn an_invalid_shape_discards_and_names_the_calculation() {
        let mut bad_output = record(good_operation(), 7);
        bad_output.id = 60;
        bad_output.outputs = vec![OutputRefRecord {
            series: 5,
            key: "corrected".into(),
            index: None,
            column: None,
        }];
        let mut no_series = record(good_operation(), 7);
        no_series.id = 61;
        no_series.series.clear();
        let mut bad_results = record(good_operation(), 7);
        bad_results.id = 62;
        bad_results.results = vec![SeriesResultRecord::default(); 2];
        let mut fine = record(good_operation(), 7);
        fine.id = 63;
        fine.results = vec![SeriesResultRecord::default()];
        let (kept, discarded) =
            records_to_calculations(vec![bad_output, no_series, bad_results, fine], 1000);
        assert_eq!(kept.keys().map(|k| k.raw()).collect::<Vec<_>>(), vec![63]);
        assert_eq!(
            discarded.iter().map(|d| d.id).collect::<Vec<_>>(),
            vec![60, 61, 62]
        );
    }

    /// #309 (F4): two calculations with the same identifier, the first is kept and
    /// the second counted among the discarded.
    #[test]
    fn a_duplicate_identifier_keeps_the_first_and_counts_the_second() {
        let mut a = record(good_operation(), 7);
        a.id = 70;
        let mut b = record(good_operation(), 8);
        b.id = 70;
        let (kept, discarded) = records_to_calculations(vec![a.clone(), b.clone()], 1000);
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[&NodeId::from_raw(70)].series[0].y, NodeId::from_raw(7));
        assert_eq!(discarded.iter().map(|d| d.id).collect::<Vec<_>>(), vec![70]);
        // Same rule for the correction traces: A kept, B discarded.
        let (kept, discarded) = records_to_corrections(vec![trace(a), trace(b)], 1000);
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[&NodeId::from_raw(70)].series[0].y, NodeId::from_raw(7));
        assert_eq!(discarded.len(), 1);
        // First = another operation than a correction: the discarded one is named
        // after the first record, not after the duplicate.
        let integrate = tree_to_value(
            &Operation::Integrate {
                spec: stokesforge_analysis::integrate::IntegrateSpec {
                    ranges: vec![],
                    under: stokesforge_analysis::integrate::Under::Zero,
                    cumulative: false,
                    normalize: false,
                },
            }
            .to_tree(),
        );
        let mut first = record(integrate, 7);
        first.id = 71;
        let mut second = record(good_operation(), 7);
        second.id = 71;
        let (kept, discarded) = records_to_corrections(vec![trace(first), trace(second)], 1000);
        assert!(kept.is_empty());
        assert!(discarded
            .iter()
            .any(|d| d.kind.as_deref() == Some("integrate")));
        assert_eq!(discarded.len(), 2);
    }

    /// #309: a discarded correction trace is counted, whether it is
    /// unreadable, out of bounds, of invalid shape or of another operation; the
    /// other traces stay loaded.
    #[test]
    fn a_discarded_trace_is_counted() {
        let junk = Value::Map(vec![(Value::from("kind"), Value::from("teleportation"))]);
        let fit = tree_to_value(
            &Operation::Integrate {
                spec: stokesforge_analysis::integrate::IntegrateSpec {
                    ranges: vec![],
                    under: stokesforge_analysis::integrate::Under::Zero,
                    cumulative: false,
                    normalize: false,
                },
            }
            .to_tree(),
        );
        let mut unreadable = record(junk, 7);
        unreadable.id = 70;
        let mut out_of_bound = record(good_operation(), 5000);
        out_of_bound.id = 71;
        let mut not_a_correction = record(fit, 7);
        not_a_correction.id = 72;
        let mut healthy = record(good_operation(), 7);
        healthy.id = 73;
        let (kept, discarded) = records_to_corrections(
            vec![unreadable, out_of_bound, not_a_correction, healthy]
                .into_iter()
                .map(trace)
                .collect(),
            1000,
        );
        assert_eq!(kept.keys().map(|k| k.raw()).collect::<Vec<_>>(), vec![73]);
        assert_eq!(
            discarded.iter().map(|d| d.id).collect::<Vec<_>>(),
            vec![70, 71, 72]
        );
    }

    #[test]
    fn an_unknown_warning_is_ignored() {
        let mut r = record(good_operation(), 7);
        r.results = vec![SeriesResultRecord {
            warnings: vec![
                WarningRecord {
                    kind: "from_the_future".into(),
                    count: 0,
                },
                WarningRecord {
                    kind: "irf_step_mismatch".into(),
                    count: 0,
                },
            ],
            ..Default::default()
        }];
        let (kept, _) = records_to_calculations(vec![r], 1000);
        let calc = kept.values().next().unwrap();
        assert_eq!(calc.results[0].warnings, vec![Warning::IrfStepMismatch]);
    }

    /// Each variant of `Warning`, exhaustive `match` without `_`: a variant
    /// added does not compile here until it has its transport line.
    fn rank(w: &Warning) -> usize {
        match w {
            Warning::SignalStepNotUniform => 0,
            Warning::IrfStepMismatch => 1,
            Warning::NegativeClipped(_) => 2,
            Warning::NotConverged => 3,
            Warning::Undetermined(_) => 4,
            Warning::IrfTruncated => 5,
            Warning::BridgedReferenceGap(_) => 6,
            Warning::DivisionByZero(_) => 7,
            Warning::ShiftedX => 8,
        }
    }

    fn all_warnings() -> Vec<Warning> {
        let mut all = vec![
            Warning::SignalStepNotUniform,
            Warning::IrfStepMismatch,
            Warning::NotConverged,
            Warning::IrfTruncated,
            Warning::ShiftedX,
        ];
        for n in [0, 1, 2, 3, 17, u32::MAX as usize, usize::MAX] {
            all.push(Warning::NegativeClipped(n));
            all.push(Warning::Undetermined(n));
            all.push(Warning::BridgedReferenceGap(n));
            all.push(Warning::DivisionByZero(n));
        }
        all
    }

    #[test]
    fn every_warning_round_trips_through_the_records() {
        let all = all_warnings();
        let ranks: std::collections::BTreeSet<usize> = all.iter().map(rank).collect();
        assert_eq!(ranks.len(), 9, "all the variants are exercised");
        for w in &all {
            assert_eq!(record_to_warning(&warning_to_record(w)).as_ref(), Some(w));
        }
    }

    #[test]
    fn the_file_names_of_the_warnings_are_frozen() {
        // These strings are the `.stf` format: renaming them breaks the files.
        let expected = [
            (Warning::SignalStepNotUniform, "signal_step_not_uniform", 0),
            (Warning::IrfStepMismatch, "irf_step_mismatch", 0),
            (Warning::NegativeClipped(4), "negative_clipped", 4),
            (Warning::NotConverged, "not_converged", 0),
            (Warning::Undetermined(2), "undetermined", 2),
            (Warning::IrfTruncated, "irf_truncated", 0),
            (Warning::BridgedReferenceGap(3), "bridged_reference_gap", 3),
            (Warning::DivisionByZero(1), "division_by_zero", 1),
            (Warning::ShiftedX, "shifted_x", 0),
        ];
        for (w, kind, count) in expected {
            let r = warning_to_record(&w);
            assert_eq!((r.kind.as_str(), r.count), (kind, count), "{w:?}");
        }
    }

    #[test]
    fn an_unknown_warning_does_not_take_the_place_of_a_known_one_and_the_order_is_kept() {
        let rec = |kind: &str, count| WarningRecord {
            kind: kind.into(),
            count,
        };
        let mut r = record(good_operation(), 7);
        r.results = vec![SeriesResultRecord {
            warnings: vec![
                rec("future_first", 9),
                rec("not_converged", 5),
                rec("Not_Converged", 0),
                rec("", 0),
                rec("undetermined", 3),
                rec("not_converged ", 0),
                rec("future_last", 1),
            ],
            ..Default::default()
        }];
        let (kept, discarded) = records_to_calculations(vec![r], 1000);
        assert!(discarded.is_empty(), "the calculation is not dropped");
        let calc = kept.values().next().unwrap();
        assert_eq!(
            calc.results[0].warnings,
            vec![Warning::NotConverged, Warning::Undetermined(3)]
        );
    }
}

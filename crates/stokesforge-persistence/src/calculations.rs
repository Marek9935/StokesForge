//! Calculs d'analyse (#181) : enregistrement sur disque et conversion.
//!
//! L'opération traverse le fichier sous la forme d'un arbre générique
//! (`stokesforge_analysis::tree::Tree`, converti en `rmpv::Value`) : un réglage
//! nouveau ne change donc pas le schéma. Lecture tolérante (invariant 10) :
//! un calcul dont l'opération est illisible est écarté — ses colonnes restent
//! des colonnes ordinaires —, une valeur de résultat ou un avertissement
//! inconnu est ignoré.

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

/// Valeur par défaut d'un booléen actif quand le champ manque au fichier.
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
    /// Boîte de dialogue quand R² est faible (#288) ; active par défaut.
    #[serde(default = "default_true")]
    pub(crate) low_r2_dialog: bool,
    /// `"manual"` ou `"input_missing"` ; un échec de recalcul s'écrit
    /// `"manual"` (le prochain recalcul retrouve l'erreur).
    #[serde(default)]
    pub(crate) stale: Option<String>,
    /// Empreinte du contenu des colonnes d'entrée à l'enregistrement
    /// (A1, #316 : `Calculation::inputs_digest`). Absente d'un fichier plus
    /// ancien, ou quand une entrée manquait : le calcul est alors cru tel quel.
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

// ── Arbre d'opération ↔ valeur MessagePack ──────────────────────────────

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

// ── Modèle → enregistrement ─────────────────────────────────────────────

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

/// `digest_in` : l'arbre où lire les entrées pour en écrire l'empreinte
/// (`None` pour les traces de correction, qui ne se périment pas).
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

/// Clé de la « sortie » d'une trace de correction qui porte la copie de
/// l'original d'une colonne remplacée (#248).
const ORIGINAL_KEY: &str = "original";

/// Même transport d'opération/résultats, avec un champ de fichier distinct :
/// une trace ne devient jamais un calcul actif à la lecture.
pub(crate) fn corrections_to_records(tree: &ModelTree) -> Vec<CalculationRecord> {
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
                    // Les copies des originaux (#248) voyagent comme des
                    // sorties de clé `original` : la série dit quelle colonne
                    // fut remplacée. Aucun schéma de plus.
                    outputs: trace
                        .originals
                        .iter()
                        .filter_map(|(column, copy)| {
                            let series = trace.series.iter().position(|s| s.y == *column)?;
                            Some(OutputRef {
                                series,
                                key: ORIGINAL_KEY.to_string(),
                                index: None,
                                column: Some(*copy),
                            })
                        })
                        .collect(),
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
}

/// Rend les traces gardées et celles écartées : une trace illisible (ou qui
/// échoue à la validation), ou dont l'opération n'est pas une correction,
/// est comptée (#309) au lieu de disparaître sans bruit.
pub(crate) fn records_to_corrections(
    records: Vec<CalculationRecord>,
    max_id: u64,
) -> (
    BTreeMap<NodeId, Arc<CorrectionRecord>>,
    Vec<DiscardedCalculation>,
) {
    // Deux enregistrements de même id : le premier fait foi (#309, F4).
    let mut named: BTreeMap<u64, DiscardedCalculation> = BTreeMap::new();
    for r in &records {
        named
            .entry(r.id)
            .or_insert_with(|| discarded_calculation(r));
    }
    let (kept, mut discarded) = records_to_calculations(records, max_id);
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
                originals: c
                    .outputs
                    .iter()
                    .filter(|o| o.key == ORIGINAL_KEY)
                    .filter_map(|o| Some((c.series.get(o.series)?.y, o.column?)))
                    .collect(),
            }),
        );
    }
    (out, discarded)
}

// ── Enregistrement → modèle ─────────────────────────────────────────────

/// Un identifiant au-delà de `max` écarte le calcul entier (même borne que
/// les nœuds, B1) ; l'opération illisible, ou hors plafond (`validate`), aussi. Rend les calculs gardés et la
/// liste de ceux qui sont écartés (#278) : un enregistrement illisible n'est
/// pas conservé, mais il n'est plus perdu sans bruit.
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

/// Calculs gardés, calculs écartés, empreintes d'entrées écrites dans le fichier.
type ConvertedCalculations = (
    BTreeMap<NodeId, Arc<Calculation>>,
    Vec<DiscardedCalculation>,
    BTreeMap<NodeId, u64>,
);

/// Comme [`records_to_calculations`], avec en plus l'empreinte d'entrées
/// écrite dans le fichier pour chaque calcul **gardé** qui en portait une.
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
        // `from_tree` ne juge que la forme : un `NaN` lu tel quel s'y glisse.
        // `validate` est le même filtre que celui de `add_calculation` (core) :
        // un calcul qui l'échoue n'a jamais pu être enregistré par
        // l'application, c'est un fichier modifié à la main (#291).
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
        // Même règle de forme qu'à l'écriture (core, #309).
        if calc.validate_shape().is_err() {
            discarded.push(discarded_calculation(&r));
            continue;
        }
        // Un identifiant déjà gardé : le premier fait foi, le second est
        // compté parmi les écartés au lieu de le remplacer sans bruit (#309).
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

/// A1 (#316) : confronte, **sans rien recalculer**, l'empreinte écrite dans le
/// fichier au contenu des entrées tel que le fichier le porte. Un calcul dont
/// l'empreinte diffère est marqué périmé (`Manual`) ; ses valeurs restent
/// celles du fichier. Un calcul sans empreinte (fichier ancien, entrée
/// absente à l'écriture) ou dont une entrée manque aujourd'hui est laissé tel
/// quel : l'invariant 10 veut une dégradation, jamais un échec.
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

/// Ce qu'on sait d'un calcul qu'on écarte : son identifiant, et la famille
/// d'opération quand l'enregistrement la porte encore lisiblement.
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
    fn une_operation_illisible_ecarte_le_calcul_sans_echec() {
        let (kept, discarded) = records_to_calculations(vec![record(good_operation(), 7)], 1000);
        assert_eq!(kept.len(), 1);
        assert!(discarded.is_empty());
        let junk = Value::Map(vec![(Value::from("kind"), Value::from("teleportation"))]);
        let (kept, discarded) = records_to_calculations(vec![record(junk, 7)], 1000);
        assert!(kept.is_empty());
        // L'écart est signalé, avec la famille quand elle se lit (#278).
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
    fn un_identifiant_hors_borne_ecarte_le_calcul() {
        let (kept, discarded) = records_to_calculations(vec![record(good_operation(), 5000)], 1000);
        assert!(kept.is_empty());
        assert_eq!(
            discarded,
            vec![DiscardedCalculation {
                id: 50,
                kind: Some("correct".into())
            }],
            "l'identifiant hors borne est signalé, famille lue"
        );
        // Pile sur la borne : gardé, rien à signaler.
        let (kept, discarded) = records_to_calculations(vec![record(good_operation(), 1000)], 1000);
        assert_eq!(kept.len(), 1);
        assert!(discarded.is_empty());
        // Chaque identifiant de l'enregistrement est borné : le sien, ceux d'une
        // sortie, d'un x, d'un sigma, d'une référence ; la borne ne fait pas
        // écarter la voisine saine.
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
            "dans l'ordre des enregistrements"
        );
    }

    #[test]
    fn plusieurs_calculs_ecartes_sont_tous_listes_et_les_sains_gardes_dans_l_ordre() {
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

    /// #309 : la forme d'un calcul est revérifiée à la lecture (même règle que
    /// l'écriture, core) ; l'écarté est nommé, jamais une panique.
    #[test]
    fn une_forme_invalide_ecarte_et_nomme_le_calcul() {
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

    /// #309 (F4) : deux calculs de même identifiant, le premier est gardé et
    /// le second compté parmi les écartés.
    #[test]
    fn un_doublon_d_identifiant_garde_le_premier_et_compte_le_second() {
        let mut a = record(good_operation(), 7);
        a.id = 70;
        let mut b = record(good_operation(), 8);
        b.id = 70;
        let (kept, discarded) = records_to_calculations(vec![a.clone(), b.clone()], 1000);
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[&NodeId::from_raw(70)].series[0].y, NodeId::from_raw(7));
        assert_eq!(discarded.iter().map(|d| d.id).collect::<Vec<_>>(), vec![70]);
        // Même règle pour les traces de correction : A gardée, B écartée.
        let (kept, discarded) = records_to_corrections(vec![a, b], 1000);
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[&NodeId::from_raw(70)].series[0].y, NodeId::from_raw(7));
        assert_eq!(discarded.len(), 1);
        // Premier = autre opération qu'une correction : l'écarté est nommé
        // d'après le premier enregistrement, pas d'après le doublon.
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
        let (kept, discarded) = records_to_corrections(vec![first, second], 1000);
        assert!(kept.is_empty());
        assert!(discarded
            .iter()
            .any(|d| d.kind.as_deref() == Some("integrate")));
        assert_eq!(discarded.len(), 2);
    }

    /// #309 : une trace de correction écartée est comptée, qu'elle soit
    /// illisible, hors borne, de forme invalide ou d'une autre opération ; les
    /// autres traces restent chargées.
    #[test]
    fn une_trace_ecartee_est_comptee() {
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
            vec![unreadable, out_of_bound, not_a_correction, healthy],
            1000,
        );
        assert_eq!(kept.keys().map(|k| k.raw()).collect::<Vec<_>>(), vec![73]);
        assert_eq!(
            discarded.iter().map(|d| d.id).collect::<Vec<_>>(),
            vec![70, 71, 72]
        );
    }

    #[test]
    fn un_avertissement_inconnu_est_ignore() {
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

    /// Chaque variante de `Warning`, `match` exhaustif sans `_` : une variante
    /// ajoutée ne compile pas ici tant qu'elle n'a pas sa ligne de transport.
    fn rang(w: &Warning) -> usize {
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

    fn tous_les_avertissements() -> Vec<Warning> {
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
    fn tout_avertissement_fait_l_aller_retour_des_enregistrements() {
        let all = tous_les_avertissements();
        let rangs: std::collections::BTreeSet<usize> = all.iter().map(rang).collect();
        assert_eq!(rangs.len(), 9, "toutes les variantes sont éprouvées");
        for w in &all {
            assert_eq!(record_to_warning(&warning_to_record(w)).as_ref(), Some(w));
        }
    }

    #[test]
    fn les_noms_de_fichier_des_avertissements_sont_figes() {
        // Ces chaînes sont le format `.stf` : les renommer casse les fichiers.
        let attendu = [
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
        for (w, kind, count) in attendu {
            let r = warning_to_record(&w);
            assert_eq!((r.kind.as_str(), r.count), (kind, count), "{w:?}");
        }
    }

    #[test]
    fn un_avertissement_inconnu_n_en_prend_pas_un_connu_et_l_ordre_est_garde() {
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
        assert!(discarded.is_empty(), "le calcul n'est pas écarté");
        let calc = kept.values().next().unwrap();
        assert_eq!(
            calc.results[0].warnings,
            vec![Warning::NotConverged, Warning::Undetermined(3)]
        );
    }
}

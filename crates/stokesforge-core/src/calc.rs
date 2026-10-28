//! The **analysis calculations** of the project (work package #181): the link between
//! computed columns, the columns they come from and the operation that
//! produced them.
//!
//! # Where it lives, and why
//!
//! A [`Calculation`] is not a node of the tree — it has no name, no parent,
//! no window — but a record carried by [`Tree`] next to the saved
//! colors (`Tree::saved_colors`): so it travels with the session
//! snapshot, the rollback and the `.stf` file with no separate path. Its
//! identity is a [`NodeId`] from the same allocator as the nodes (never
//! reused, invariant 7), but it names no node of the map.
//!
//! The trace of the operation of a computed column ("this column comes from such
//! calculation") is **not** stored on the column: it is read from
//! `Calculation::outputs` ([`Tree::calculation_of_column`]). A single source of
//! truth prevents a column copy or a move from leaving
//! two traces in disagreement.
//!
//! # Dangling links
//!
//! Deleting a column does not edit the calculations that cite it: the reference
//! becomes dangling, and it is the read that handles it. A deleted **input**
//! makes the calculation stale ([`Staleness::InputMissing`]); a deleted
//! **output** stays in `outputs` with `column` pointing to a missing node,
//! and reads back as "output deleted" (M8 C). A single exception, so
//! that nothing lingers: `Project::remove` removes the calculation whose **all**
//! input Y columns have disappeared (undoing the deletion brings it back with
//! the tree).
//!
//! # What is written outside history
//!
//! The recalculation (`stokesforge-session::recalc`) rewrites the output columns,
//! the results and the stale state through [`Project::refresh_calculation`] and
//! [`Project::set_calculation_stale`], as it rewrites formula columns:
//! never a history entry. Creating, changing the
//! parameters of and deleting a calculation are, for their part, undoable commands
//! (`Command::AddCalculation`, `UpdateCalculation`,
//! `RemoveCalculation`).

use std::sync::Arc;

use stokesforge_analysis::ops::{
    self, AuxInput, Operation, OperationKind, SeriesInput, Value, Warning,
};
use stokesforge_analysis::AnalysisError;

use crate::column::{Column, ColumnMeta, ColumnType, MixedCell, ValueBlock};
use crate::error::{CoreError, CoreResult};
use crate::id::NodeId;
use crate::project::Project;
use crate::tree::Tree;

/// The columns of a processed curve: its Y, its paired X and, for a
/// weighted fit, its standard-deviation column.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SeriesRef {
    pub x: Option<NodeId>,
    pub y: NodeId,
    pub sigma: Option<NodeId>,
}

/// The auxiliary column of a calculation: the reference of a correction, the IRF of a
/// deconvolution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuxRef {
    pub x: Option<NodeId>,
    pub y: NodeId,
}

/// An output column: which series it comes from, which output of the
/// operation it carries (`OutColumn::key` + `index`), and where it is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutputRef {
    pub series: usize,
    pub key: String,
    pub index: Option<usize>,
    /// The column written. A node that no longer exists: output deleted.
    pub column: Option<NodeId>,
}

/// A computed value kept in the project.
///
/// Same content as [`stokesforge_analysis::ops::Value`], but equality treats
/// two `NaN` as equal: a tree restored by undo must be
/// **identical** to the one before, even when a result is `NaN`.
#[derive(Debug, Clone)]
pub struct ResultValue {
    pub key: String,
    pub index: Option<usize>,
    pub value: f64,
    pub error: Option<f64>,
}

fn same_float(a: f64, b: f64) -> bool {
    a.to_bits() == b.to_bits() || (a.is_nan() && b.is_nan())
}

impl PartialEq for ResultValue {
    fn eq(&self, other: &Self) -> bool {
        self.key == other.key
            && self.index == other.index
            && same_float(self.value, other.value)
            && match (self.error, other.error) {
                (Some(a), Some(b)) => same_float(a, b),
                (None, None) => true,
                _ => false,
            }
    }
}

impl From<&Value> for ResultValue {
    fn from(v: &Value) -> Self {
        Self {
            key: v.key.clone(),
            index: v.index,
            value: v.value,
            error: v.error,
        }
    }
}

/// The values of a processed series.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SeriesResult {
    pub values: Vec<ResultValue>,
    /// Rows discarded (X or Y not finite).
    pub skipped: usize,
    pub warnings: Vec<Warning>,
}

/// Why the data of a calculation is not up to date (M9 a).
#[derive(Debug, Clone, PartialEq)]
pub enum Staleness {
    /// Manual recalculation: a source changed and nobody reran it.
    Manual,
    /// An input column no longer exists.
    InputMissing,
    /// The recalculation failed; the last valid values are kept.
    Failed(AnalysisError),
}

/// An analysis calculation recorded in the project.
#[derive(Debug, Clone, PartialEq)]
pub struct Calculation {
    pub operation: Operation,
    pub series: Vec<SeriesRef>,
    pub aux: Option<AuxRef>,
    pub outputs: Vec<OutputRef>,
    /// One entry per series, or empty when results are not kept.
    pub results: Vec<SeriesResult>,
    /// "Keep in Results" (M5): the values are kept and shown in the
    /// Results pane. Otherwise the trace only serves the columns.
    pub keep_results: bool,
    /// Rerun automatically when a source changes (M9 b).
    pub auto_recalc: bool,
    /// Opens a dialog box when the R² of a fit is very low
    /// (#288); specific to the calculation, undoable and saved. On by default.
    pub low_r2_dialog: bool,
    pub stale: Option<Staleness>,
}

impl Calculation {
    /// The columns it reads: series and auxiliary.
    pub fn input_columns(&self) -> Vec<NodeId> {
        let mut out = Vec::new();
        for s in &self.series {
            out.extend(s.x);
            out.push(s.y);
            out.extend(s.sigma);
        }
        if let Some(a) = &self.aux {
            out.extend(a.x);
            out.push(a.y);
        }
        out
    }

    /// **Stable fingerprint of the content** of the input columns (A1, #316): type
    /// and values of each, in the order of [`Calculation::input_columns`].
    /// `None` if a column is missing or is not a column.
    ///
    /// Written into the file on save and compared on read,
    /// without recalculation, to mark stale a calculation whose input changed
    /// outside StokesForge. It **never** contains a column version
    /// (not persisted) nor an identifier: the same content under two
    /// `NodeId`s gives the same value. FNV-1a written by hand, because
    /// `DefaultHasher` guarantees no stability from one version to the next.
    pub fn inputs_digest(&self, tree: &Tree) -> Option<u64> {
        let mut hash = Fnv1a::new();
        let inputs = self.input_columns();
        hash.write(&(inputs.len() as u64).to_le_bytes());
        for id in inputs {
            let column = tree.node(id)?.as_column()?;
            hash.write(&[match column.column_type() {
                ColumnType::Numeric => 0,
                ColumnType::DateTime => 1,
                ColumnType::Mixed => 2,
            }]);
            match column.values() {
                crate::column::ColumnValues::Float(values) => {
                    hash.write(&[0]);
                    hash.write(&(values.len() as u64).to_le_bytes());
                    for v in values.iter() {
                        hash.write(&v.to_bits().to_le_bytes());
                    }
                }
                crate::column::ColumnValues::Mixed(cells) => {
                    hash.write(&[1]);
                    hash.write(&(cells.len() as u64).to_le_bytes());
                    for cell in cells.iter() {
                        match cell {
                            MixedCell::Empty => hash.write(&[0]),
                            MixedCell::Number(v) => {
                                hash.write(&[1]);
                                hash.write(&v.to_bits().to_le_bytes());
                            }
                            MixedCell::Text(t) => {
                                hash.write(&[2]);
                                hash.write(&(t.len() as u64).to_le_bytes());
                                hash.write(t.as_bytes());
                            }
                        }
                    }
                }
            }
        }
        Some(hash.finish())
    }

    /// The output columns that still exist in `tree`.
    pub fn live_outputs<'a>(
        &'a self,
        tree: &'a crate::tree::Tree,
    ) -> impl Iterator<Item = &'a OutputRef> + 'a {
        self.outputs
            .iter()
            .filter(move |o| o.column.is_some_and(|c| tree.node(c).is_some()))
    }
}

/// FNV-1a 64 bits: the fingerprint of [`Calculation::inputs_digest`] is written
/// into a file, so its algorithm must never change under our feet.
struct Fnv1a(u64);

impl Fnv1a {
    fn new() -> Self {
        Self(0xcbf2_9ce4_8422_2325)
    }

    fn write(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.0 ^= u64::from(b);
            self.0 = self.0.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }

    fn finish(&self) -> u64 {
        self.0
    }
}

/// A column to create or rewrite for a calculation.
#[derive(Debug, Clone, PartialEq)]
pub struct OutputDraft {
    pub series: usize,
    pub key: String,
    pub index: Option<usize>,
    /// Metadata of the column (long name, comment, unit).
    pub meta: ColumnMeta,
    pub data: Vec<f64>,
}

/// Everything needed to record a calculation: the record and the
/// output columns with their values.
#[derive(Debug, Clone, PartialEq)]
pub struct CalculationDraft {
    pub operation: Operation,
    pub series: Vec<SeriesRef>,
    pub aux: Option<AuxRef>,
    pub outputs: Vec<OutputDraft>,
    pub results: Vec<SeriesResult>,
    pub keep_results: bool,
    pub auto_recalc: bool,
    /// See [`Calculation::low_r2_dialog`].
    pub low_r2_dialog: bool,
}

/// Immutable trace of a correction that replaced its source data (#212).
/// It creates no calculation link: the columns stay ordinary.
#[derive(Debug, Clone, PartialEq)]
pub struct CorrectionRecord {
    pub operation: Operation,
    pub series: Vec<SeriesRef>,
    /// The frozen reference of a correction by a column (#248): a copy
    /// of the hidden workbook, never the original column.
    pub aux: Option<AuxRef>,
    pub results: Vec<SeriesResult>,
    /// `(replaced column, copy of its original)`: what is needed to restore it
    /// ("Restore original data", #248). The copies live in the hidden
    /// analysis workbook.
    pub originals: Vec<(NodeId, NodeId)>,
}

/// Cap on the series of a single calculation (a batch of spectra).
pub const MAX_CALCULATION_SERIES: usize = 10_000;

/// Shape of a calculation: sole source of the structural rules, called
/// at write time (`CalculationDraft`) as at read time of a `.stf` (#309).
/// `output_series`: the series rank of each output; `results`: the
/// number of results kept.
pub fn validate_calculation_shape(
    series: usize,
    output_series: impl IntoIterator<Item = usize>,
    results: usize,
) -> CoreResult<()> {
    if series == 0 || series > MAX_CALCULATION_SERIES {
        return Err(CoreError::Structure(format!(
            "a calculation needs between 1 and {MAX_CALCULATION_SERIES} series, got {series}"
        )));
    }
    if output_series.into_iter().any(|s| s >= series) {
        return Err(CoreError::Structure(
            "a calculation output refers to a series that does not exist".to_string(),
        ));
    }
    if results != 0 && results != series {
        return Err(CoreError::Structure(
            "a calculation keeps one result per series".to_string(),
        ));
    }
    Ok(())
}

impl Calculation {
    /// Checks the shape (see [`validate_calculation_shape`]).
    pub fn validate_shape(&self) -> CoreResult<()> {
        validate_calculation_shape(
            self.series.len(),
            self.outputs.iter().map(|o| o.series),
            self.results.len(),
        )
    }
}

impl CalculationDraft {
    /// Prepares the replacement of the original data (C4, #313): an output
    /// has the length of X, shorter than Y when X is. It is padded
    /// with `NaN` up to the length of the source Y column; these rows have
    /// no abscissa and keep their original cell at write time. To call
    /// before `ReplaceAnalysisData`, whose length guard stays strict.
    pub fn complete_for_replacement(&mut self, tree: &Tree) {
        for output in &mut self.outputs {
            let Some(source) = self.series.get(output.series) else {
                continue;
            };
            let len = tree
                .node(source.y)
                .and_then(|n| n.as_column())
                .map_or(0, Column::len);
            if output.data.len() < len {
                output.data.resize(len, f64::NAN);
            }
        }
    }

    fn validate(&self) -> CoreResult<()> {
        validate_calculation_shape(
            self.series.len(),
            self.outputs.iter().map(|o| o.series),
            self.results.len(),
        )?;
        // An invalid operation (non-finite, cap) is not recorded: it
        // would be written then lost when the `.stf` is reread (#278).
        self.operation
            .validate()
            .map_err(|e| CoreError::Structure(e.to_string()))?;
        Ok(())
    }
}

/// The key of the output that replaces the Ys of a source: the corrected curve
/// of a correction or a baseline, the smoothed curve of a smoothing.
fn replacement_key(operation: &Operation) -> &'static str {
    match operation {
        Operation::Smooth { .. } => "smoothed",
        _ => "corrected",
    }
}

impl Project {
    /// Replaces the Ys with their correction and keeps the trace of the gesture, atomically.
    /// The client supplies the versions of all inputs read: a result
    /// prepared on a stale snapshot must never overwrite fresh data.
    pub fn replace_analysis_data(
        &mut self,
        draft: CalculationDraft,
        expected_versions: &[(NodeId, u64)],
    ) -> CoreResult<NodeId> {
        draft.validate()?;
        let mut inputs: Vec<NodeId> = draft
            .series
            .iter()
            .flat_map(|s| [Some(s.y), s.x, s.sigma].into_iter().flatten())
            .collect();
        if let Some(aux) = &draft.aux {
            inputs.push(aux.y);
            inputs.extend(aux.x);
        }
        if inputs
            .iter()
            .any(|id| !expected_versions.iter().any(|(read, _)| read == id))
        {
            return Err(CoreError::Structure(
                "a correction needs the version of every input".into(),
            ));
        }
        if !matches!(
            draft.operation,
            Operation::Baseline { .. } | Operation::Correct { .. } | Operation::Smooth { .. }
        ) {
            return Err(CoreError::Structure(
                "only a curve correction or smoothing can replace original data".into(),
            ));
        }
        for &(id, expected) in expected_versions {
            let column = self.tree().get(id)?.as_column().ok_or_else(|| {
                CoreError::Structure("a correction input must be a column".into())
            })?;
            if column.version() != expected {
                return Err(CoreError::ConcurrentModification {
                    column: id,
                    expected,
                    found: column.version(),
                });
            }
        }
        let mut writes: std::collections::BTreeMap<NodeId, ValueBlock> =
            std::collections::BTreeMap::new();
        for (rank, source) in draft.series.iter().enumerate() {
            let column = self.tree().get(source.y)?.as_column().ok_or_else(|| {
                CoreError::Structure("a correction source must be a column".into())
            })?;
            if column.meta().formula.is_some() {
                return Err(CoreError::FormulaColumnReadOnly { column: source.y });
            }
            if let Some((calculation, _)) = self.tree().calculation_of_column(source.y) {
                return Err(CoreError::CalculatedColumnReadOnly {
                    column: source.y,
                    calculation,
                });
            }
            let output = draft
                .outputs
                .iter()
                .find(|o| o.series == rank && o.key == replacement_key(&draft.operation))
                .ok_or_else(|| {
                    CoreError::Structure("a correction needs its corrected output".into())
                })?;
            if output.data.len() < column.len() {
                return Err(CoreError::Structure(
                    "a correction must preserve the source length".into(),
                ));
            }
            // Rows that cannot be used keep their original cell. A
            // NaN result on a valid row (division by zero, out-of-range
            // reference) stays NaN: it is not a missing input value.
            let cells: Vec<MixedCell> = (0..column.len())
                .map(|row| {
                    let x_ok = source.x.is_none_or(|x| {
                        self.tree()
                            .node(x)
                            .and_then(|n| n.as_column())
                            .is_some_and(|c| c.values().number_at(row).is_finite())
                    });
                    if !column.values().number_at(row).is_finite() || !x_ok {
                        column.cell(row)
                    } else {
                        MixedCell::Number(output.data[row])
                    }
                })
                .collect();
            let values = if column.column_type() == ColumnType::Mixed {
                ValueBlock::Mixed(cells)
            } else {
                ValueBlock::Float(
                    cells
                        .iter()
                        .enumerate()
                        .map(|(row, cell)| {
                            if !column.values().number_at(row).is_finite() {
                                column.values().number_at(row)
                            } else {
                                cell.as_number()
                            }
                        })
                        .collect(),
                )
            };
            if let Some(previous) = writes.get(&source.y) {
                if previous != &values {
                    return Err(CoreError::Structure(
                        "one source has conflicting corrections".into(),
                    ));
                }
            } else {
                writes.insert(source.y, values);
            }
        }
        let before = self.tree().clone();
        let result = (|| {
            let aux = self.frozen_aux(&draft.operation, draft.aux.as_ref())?;
            let mut originals = Vec::with_capacity(writes.len());
            for &id in writes.keys() {
                // A column already corrected keeps its original raw data
                // (#308): the copy of the first replacement is reused, the
                // already corrected state is not photographed.
                let copy = match self.first_original_of(id) {
                    Some(copy) => copy,
                    None => self.keep_original(id)?,
                };
                originals.push((id, copy));
            }
            for (id, values) in writes {
                self.set_values(id, 0, values)?;
            }
            let id = self.allocate_id()?;
            Arc::make_mut(self.tree_arc_mut())
                .correction_traces_mut()
                .insert(
                    id,
                    Arc::new(CorrectionRecord {
                        operation: draft.operation,
                        series: draft.series,
                        aux,
                        results: draft.results,
                        originals,
                    }),
                );
            self.gc_analysis_store();
            Ok(id)
        })();
        if result.is_err() {
            self.replace_tree(before);
        }
        result
    }

    /// The copy of the raw data of `column`, if an earlier correction
    /// already kept it: that of the trace with the smallest identifier, hence the
    /// oldest. `None` for a column never corrected.
    fn first_original_of(&self, column: NodeId) -> Option<NodeId> {
        self.tree().correction_traces().values().find_map(|trace| {
            trace
                .originals
                .iter()
                .find(|(replaced, copy)| {
                    *replaced == column
                        && self
                            .tree()
                            .node(*copy)
                            .and_then(|n| n.as_column())
                            .is_some()
                })
                .map(|(_, copy)| *copy)
        })
    }

    /// "Restore original data" (#248): gives back to each column replaced by
    /// the correction `trace` its **raw** type and values (those from before
    /// the very first correction, #308), **overwrites** what was typed in
    /// since, removes the trace and the copies that are no longer needed. A restored
    /// column also leaves all the other traces that cited it (a
    /// trace left with no column disappears): restoring B after A gives back
    /// the raw data, there is no "A" correction left to restore.
    /// A column deleted since is left aside. Atomic.
    pub fn restore_original_data(&mut self, trace: NodeId) -> CoreResult<()> {
        let record = self
            .tree()
            .correction_traces()
            .get(&trace)
            .cloned()
            .ok_or(CoreError::UnknownNode(trace))?;
        let before = self.tree().clone();
        let result = (|| {
            for (column, copy) in &record.originals {
                if self
                    .tree()
                    .node(*column)
                    .and_then(|n| n.as_column())
                    .is_some()
                {
                    self.restore_from_copy(*column, *copy)?;
                }
            }
            let restored: Vec<NodeId> = record.originals.iter().map(|(c, _)| *c).collect();
            let traces = Arc::make_mut(self.tree_arc_mut()).correction_traces_mut();
            traces.remove(&trace);
            let others: Vec<NodeId> = traces.keys().copied().collect();
            for other in others {
                let Some(existing) = traces.get(&other).cloned() else {
                    continue;
                };
                if !existing
                    .originals
                    .iter()
                    .any(|(column, _)| restored.contains(column))
                {
                    continue;
                }
                let mut stripped = (*existing).clone();
                stripped
                    .originals
                    .retain(|(column, _)| !restored.contains(column));
                if stripped.originals.is_empty() {
                    traces.remove(&other);
                } else {
                    traces.insert(other, Arc::new(stripped));
                }
            }
            self.gc_analysis_store();
            Ok(())
        })();
        if result.is_err() {
            self.replace_tree(before);
        }
        result
    }

    /// The reference the calculation will read: frozen in the hidden workbook for a
    /// correction by a column (#248), as is for the other
    /// operations.
    fn frozen_aux(
        &mut self,
        operation: &Operation,
        aux: Option<&AuxRef>,
    ) -> CoreResult<Option<AuxRef>> {
        match (operation, aux) {
            (Operation::Correct { .. }, Some(aux)) => self.freeze_reference(aux).map(Some),
            (_, aux) => Ok(aux.cloned()),
        }
    }

    /// Records a calculation: creates its output columns **right after the
    /// Y column of each series** (in the order of the outputs), then
    /// the record. Atomic: a failure leaves the tree intact.
    pub fn add_calculation(&mut self, draft: CalculationDraft) -> CoreResult<NodeId> {
        draft.validate()?;
        let before = self.tree().clone();
        let result = self.add_calculation_inner(draft);
        if result.is_err() {
            self.replace_tree(before);
        }
        result
    }

    fn add_calculation_inner(&mut self, draft: CalculationDraft) -> CoreResult<NodeId> {
        let aux = self.frozen_aux(&draft.operation, draft.aux.as_ref())?;
        let mut last_after: Vec<NodeId> = draft.series.iter().map(|s| s.y).collect();
        let mut outputs = Vec::with_capacity(draft.outputs.len());
        for out in &draft.outputs {
            let column = self.insert_output_column(&mut last_after, out)?;
            outputs.push(OutputRef {
                series: out.series,
                key: out.key.clone(),
                index: out.index,
                column: Some(column),
            });
        }
        let id = self.allocate_id()?;
        let calc = Calculation {
            operation: draft.operation,
            series: draft.series,
            aux,
            outputs,
            results: if draft.keep_results {
                draft.results
            } else {
                Vec::new()
            },
            keep_results: draft.keep_results,
            auto_recalc: draft.auto_recalc,
            low_r2_dialog: draft.low_r2_dialog,
            stale: None,
        };
        Arc::make_mut(self.tree_arc_mut())
            .calculations_mut()
            .insert(id, Arc::new(calc));
        self.gc_analysis_store();
        Ok(id)
    }

    /// Creates the column of `out`, right after `last_after[out.series]`.
    fn insert_output_column(
        &mut self,
        last_after: &mut [NodeId],
        out: &OutputDraft,
    ) -> CoreResult<NodeId> {
        let anchor = last_after[out.series];
        let sheet = self
            .tree()
            .parent(anchor)?
            .ok_or_else(|| CoreError::Structure("the source column has no sheet".to_string()))?;
        let position = self
            .tree()
            .children(sheet)?
            .iter()
            .position(|&c| c == anchor)
            .ok_or(CoreError::UnknownNode(anchor))?;
        let mut column = Column::from_f64(&out.data);
        column.set_meta(out.meta.clone());
        let id = self.add_column(sheet, Some(position + 1), None, column)?;
        last_after[out.series] = id;
        Ok(id)
    }

    /// Replaces the parameters, columns and results of an existing
    /// calculation. An output already present (same series, same key, same rank) is
    /// **rewritten in place**; a new one is created; an output that no longer exists
    /// in `draft` has its column deleted.
    pub fn update_calculation(&mut self, id: NodeId, draft: CalculationDraft) -> CoreResult<()> {
        draft.validate()?;
        let current = self
            .tree()
            .calculation(id)
            .cloned()
            .ok_or(CoreError::UnknownNode(id))?;
        let before = self.tree().clone();
        let result = self.update_calculation_inner(id, &current, draft);
        if result.is_err() {
            self.replace_tree(before);
        }
        result
    }

    fn update_calculation_inner(
        &mut self,
        id: NodeId,
        current: &Calculation,
        draft: CalculationDraft,
    ) -> CoreResult<()> {
        let aux = self.frozen_aux(&draft.operation, draft.aux.as_ref())?;
        let mut last_after: Vec<NodeId> = draft.series.iter().map(|s| s.y).collect();
        let mut outputs = Vec::with_capacity(draft.outputs.len());
        let mut kept: Vec<NodeId> = Vec::new();
        for out in &draft.outputs {
            let existing = current
                .outputs
                .iter()
                .find(|o| o.series == out.series && o.key == out.key && o.index == out.index)
                .and_then(|o| o.column)
                .filter(|c| {
                    self.tree()
                        .node(*c)
                        .is_some_and(|n| n.as_column().is_some())
                });
            let column = match existing {
                Some(col) => {
                    self.write_column_data(col, &out.data)?;
                    self.set_column_meta(col, out.meta.clone())?;
                    last_after[out.series] = col;
                    col
                }
                None => self.insert_output_column(&mut last_after, out)?,
            };
            kept.push(column);
            outputs.push(OutputRef {
                series: out.series,
                key: out.key.clone(),
                index: out.index,
                column: Some(column),
            });
        }
        for old in &current.outputs {
            if let Some(col) = old.column {
                if !kept.contains(&col) && self.tree().node(col).is_some() {
                    self.remove_without_gc(col)?;
                }
            }
        }
        let calc = Calculation {
            operation: draft.operation,
            series: draft.series,
            aux,
            outputs,
            results: if draft.keep_results {
                draft.results
            } else {
                Vec::new()
            },
            keep_results: draft.keep_results,
            auto_recalc: draft.auto_recalc,
            low_r2_dialog: draft.low_r2_dialog,
            stale: None,
        };
        Arc::make_mut(self.tree_arc_mut())
            .calculations_mut()
            .insert(id, Arc::new(calc));
        self.gc_analysis_store();
        Ok(())
    }

    /// Removes a calculation. `with_columns` also deletes its output columns;
    /// otherwise they become ordinary columns ("Detach from
    /// Calculation").
    pub fn remove_calculation(&mut self, id: NodeId, with_columns: bool) -> CoreResult<()> {
        let calc = self
            .tree()
            .calculation(id)
            .cloned()
            .ok_or(CoreError::UnknownNode(id))?;
        let before = self.tree().clone();
        let result = (|| {
            if with_columns {
                for out in &calc.outputs {
                    if let Some(col) = out.column {
                        if self.tree().node(col).is_some() {
                            self.remove_without_gc(col)?;
                        }
                    }
                }
            }
            Arc::make_mut(self.tree_arc_mut())
                .calculations_mut()
                .remove(&id);
            self.gc_analysis_store();
            Ok(())
        })();
        if result.is_err() {
            self.replace_tree(before);
        }
        result
    }

    /// Rewrites the values of a column if they differ (fresh project version
    /// on each real write; nothing if the content is identical, so as
    /// not to rerun in cascade the calculations that read it).
    fn write_column_data(&mut self, column: NodeId, data: &[f64]) -> CoreResult<bool> {
        let same = self
            .tree()
            .get(column)?
            .as_column()
            .and_then(Column::numeric_view)
            .is_some_and(|cur| {
                cur.len() == data.len() && cur.iter().zip(data).all(|(a, b)| same_float(*a, *b))
            });
        if same {
            return Ok(false);
        }
        self.set_length(column, data.len())?;
        self.set_values(column, 0, ValueBlock::Float(data.to_vec()))?;
        Ok(true)
    }

    /// **Outside history** — called by the session's recalculation. Rewrites the
    /// output columns (`data`: one entry per output of `Calculation::
    /// outputs`, in their order; `None` leaves the column as is),
    /// replaces the results if the calculation keeps them, and lifts the stale state.
    /// Returns `true` if something really changed (a column, the
    /// results or the state): this is what tells the session's recalculation to
    /// go over the formulas and calculations that read these columns again.
    pub fn refresh_calculation(
        &mut self,
        id: NodeId,
        data: &[Option<Vec<f64>>],
        results: Vec<SeriesResult>,
    ) -> CoreResult<bool> {
        let calc = self
            .tree()
            .calculation(id)
            .cloned()
            .ok_or(CoreError::UnknownNode(id))?;
        let mut changed = false;
        for (out, d) in calc.outputs.iter().zip(data) {
            if let (Some(col), Some(d)) = (out.column, d) {
                if self
                    .tree()
                    .node(col)
                    .is_some_and(|n| n.as_column().is_some())
                {
                    changed |= self.write_column_data(col, d)?;
                }
            }
        }
        let mut next = (*calc).clone();
        if next.keep_results {
            next.results = results;
        }
        next.stale = None;
        if next != *calc {
            Arc::make_mut(self.tree_arc_mut())
                .calculations_mut()
                .insert(id, Arc::new(next));
            changed = true;
        }
        Ok(changed)
    }

    /// **Outside history** — like [`Project::refresh_calculation`], from
    /// the result of [`compute`]. Each computed column finds its output
    /// by (series, key, rank); an output the calculation no longer produces (fewer
    /// peaks detected) is emptied to `NaN` — the recalculation neither creates nor
    /// deletes a column, that remains the job of `UpdateCalculation`.
    pub fn refresh_calculation_from(&mut self, id: NodeId, computed: Computed) -> CoreResult<bool> {
        let calc = self
            .tree()
            .calculation(id)
            .cloned()
            .ok_or(CoreError::UnknownNode(id))?;
        let data: Vec<Option<Vec<f64>>> =
            calc.outputs
                .iter()
                .map(|out| {
                    let found = computed.columns.iter().find(|c| {
                        c.series == out.series && c.key == out.key && c.index == out.index
                    });
                    Some(match found {
                        Some(c) => c.data.clone(),
                        None => {
                            let len = out
                                .column
                                .and_then(|c| self.tree().node(c))
                                .and_then(|n| n.as_column())
                                .map_or(0, Column::len);
                            vec![f64::NAN; len]
                        }
                    })
                })
                .collect();
        self.refresh_calculation(id, &data, computed.results)
    }

    /// **Outside history** — marks a calculation stale (or up to date with `None`).
    pub fn set_calculation_stale(
        &mut self,
        id: NodeId,
        stale: Option<Staleness>,
    ) -> CoreResult<()> {
        let calc = self
            .tree()
            .calculation(id)
            .cloned()
            .ok_or(CoreError::UnknownNode(id))?;
        if calc.stale == stale {
            return Ok(());
        }
        let mut next = (*calc).clone();
        next.stale = stale;
        Arc::make_mut(self.tree_arc_mut())
            .calculations_mut()
            .insert(id, Arc::new(next));
        Ok(())
    }

    /// Replaces a whole record (persistence and tests). Returns
    /// the old one.
    pub fn restore_calculation(
        &mut self,
        id: NodeId,
        calc: Option<Calculation>,
    ) -> Option<Arc<Calculation>> {
        let map = Arc::make_mut(self.tree_arc_mut()).calculations_mut();
        match calc {
            Some(c) => map.insert(id, Arc::new(c)),
            None => map.remove(&id),
        }
    }
}

// ── Compute ─────────────────────────────────────────────────────────────

/// Why a calculation could not be run.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum ComputeError {
    /// An input column no longer exists (or is no longer a column).
    #[error("input column {0} no longer exists")]
    MissingInput(NodeId),
    /// The operation itself failed.
    #[error(transparent)]
    Analysis(#[from] AnalysisError),
}

/// A computed column, before being placed in the workbook.
#[derive(Debug, Clone, PartialEq)]
pub struct ComputedColumn {
    pub series: usize,
    pub key: String,
    pub index: Option<usize>,
    pub data: Vec<f64>,
}

/// The result of an operation on the project's columns: what is needed to build a
/// [`CalculationDraft`] (creation) or refresh a calculation (recalculation).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Computed {
    pub columns: Vec<ComputedColumn>,
    /// One entry per series.
    pub results: Vec<SeriesResult>,
}

/// The numbers of a column, `NaN` for an empty or text cell, padded
/// with `NaN` up to `len`.
fn numbers(tree: &Tree, column: NodeId, len: usize) -> Result<Vec<f64>, ComputeError> {
    let col = tree
        .node(column)
        .and_then(|n| n.as_column())
        .ok_or(ComputeError::MissingInput(column))?;
    let values = col.values();
    Ok((0..len).map(|r| values.number_at(r)).collect())
}

fn column_len(tree: &Tree, column: NodeId) -> Result<usize, ComputeError> {
    tree.node(column)
        .and_then(|n| n.as_column())
        .map(Column::len)
        .ok_or(ComputeError::MissingInput(column))
}

/// X, Y and, if needed, an uncertainty column, read row by row.
type ReadColumns = (Vec<f64>, Vec<f64>, Option<Vec<f64>>);

/// An (X, Y) pair read from `tree`. Without an X column, the abscissa is the row
/// number (starting at 1). **The length is that of X** if given,
/// otherwise that of Y (C4, #313): Y and the uncertainty are brought to that
/// length, padded with `NaN` if shorter (incomplete row,
/// discarded by the operation), truncated if longer (rows beyond
/// X have no abscissa).
fn read_pair(
    tree: &Tree,
    x: Option<NodeId>,
    y: NodeId,
    extra: Option<NodeId>,
) -> Result<ReadColumns, ComputeError> {
    let len = match x {
        Some(x) => column_len(tree, x)?,
        None => column_len(tree, y)?,
    };
    let xs = match x {
        Some(x) => numbers(tree, x, len)?,
        None => (1..=len).map(|r| r as f64).collect(),
    };
    let ys = numbers(tree, y, len)?;
    let es = extra.map(|e| numbers(tree, e, len)).transpose()?;
    Ok((xs, ys, es))
}

/// Runs `op` on the columns cited of `tree`. This is the model read
/// shared by creation (application, `pyst`) and the session's
/// recalculation; the operation itself is `stokesforge_analysis::ops::run`.
pub fn compute(
    tree: &Tree,
    op: &Operation,
    series: &[SeriesRef],
    aux: Option<&AuxRef>,
) -> Result<Computed, ComputeError> {
    let mut inputs = Vec::with_capacity(series.len());
    for s in series {
        let (x, y, sigma) = read_pair(tree, s.x, s.y, s.sigma)?;
        inputs.push(SeriesInput { x, y, sigma });
    }
    let aux_input = match aux {
        Some(a) => {
            let (x, y, _) = read_pair(tree, a.x, a.y, None)?;
            Some(AuxInput { x, y })
        }
        None => None,
    };
    let outcomes = ops::run(op, &inputs, aux_input.as_ref())?;
    let mut computed = Computed::default();
    for (rank, outcome) in outcomes.iter().enumerate() {
        for c in &outcome.columns {
            computed.columns.push(ComputedColumn {
                series: rank,
                key: c.key.to_string(),
                index: c.index,
                data: c.data.clone(),
            });
        }
        computed.results.push(SeriesResult {
            values: outcome.values.iter().map(ResultValue::from).collect(),
            skipped: outcome.skipped,
            warnings: outcome.warnings.clone(),
        });
    }
    Ok(computed)
}

/// The name of an analysis output column, **as data**: the crates
/// below put nothing into sentences. A single rule, shared by the interface
/// and `pyst` (#256): [`OutputName::of`] chooses the variant from the family,
/// the output key and the source; [`OutputName::english`] gives the raw
/// English form (scripts, logs); the application translates it through an
/// exhaustive `match` to the `analysis-output-*` keys of the `en` catalog
/// (`tests/output_names.rs` guards the agreement of the two).
///
/// `name` is the long name of the source, failing that its short name; `column` is
/// always its short name ("B").
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OutputName {
    /// The corrected Y of a baseline: `Intensity (B-corr)`.
    BaselineCorrected { name: String, column: String },
    /// The corrected Y of a correction: `Intensity (corrected)`.
    Corrected { name: String },
    /// `Baseline of B`.
    Baseline { column: String },
    /// `Cumulative integral of B`.
    Cumulative { column: String },
    /// `Normalized integral of B`.
    Normalized { column: String },
    /// `Intensity (fit)`.
    Fit { name: String },
    /// `Intensity (resid.)`.
    Residuals { name: String },
    /// `Peak 2 of B` (rank starting at 1).
    Peak { index: usize, column: String },
    /// `Background of B`.
    Background { column: String },
    /// `Sum of peaks of B`.
    PeaksSum { column: String },
    /// `Intensity (deconvolved)`.
    Deconvolved { name: String },
    /// The smoothed Y: `Intensity (smoothed)`.
    Smoothed { name: String },
    /// Fallback of a key this version does not know: the name of the source.
    Source { name: String },
}

/// The short name of a column ("B"), failing that empty.
fn short_name(tree: &Tree, column: NodeId) -> String {
    tree.node(column)
        .map(|n| n.name().to_string())
        .unwrap_or_default()
}

/// The long name of a column, or failing that its short name.
fn long_name(tree: &Tree, column: NodeId) -> String {
    tree.node(column)
        .and_then(|n| n.as_column())
        .map(|c| c.meta().long_name.clone())
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| short_name(tree, column))
}

impl OutputName {
    /// The name of output `key` (and `index`, for a peak) of `kind` on `series`.
    pub fn of(
        tree: &Tree,
        kind: OperationKind,
        series: &SeriesRef,
        key: &str,
        index: Option<usize>,
    ) -> Self {
        let column = short_name(tree, series.y);
        let name = long_name(tree, series.y);
        match (kind, key) {
            (OperationKind::Baseline, "corrected") => Self::BaselineCorrected { name, column },
            (_, "corrected") => Self::Corrected { name },
            (_, "baseline") => Self::Baseline { column },
            (_, "cumulative") => Self::Cumulative { column },
            (_, "normalized") => Self::Normalized { column },
            (_, "fit") => Self::Fit { name },
            (_, "residuals") => Self::Residuals { name },
            (_, "peak") => Self::Peak {
                index: index.unwrap_or(1),
                column,
            },
            (_, "background") => Self::Background { column },
            (_, "peaks_sum") => Self::PeaksSum { column },
            (_, "deconvolved") => Self::Deconvolved { name },
            (_, "smoothed") => Self::Smoothed { name },
            _ => Self::Source { name },
        }
    }

    /// The raw English form, identical to the application's `en` catalog.
    pub fn english(&self) -> String {
        match self {
            Self::BaselineCorrected { name, column } => format!("{name} ({column}-corr)"),
            Self::Corrected { name } => format!("{name} (corrected)"),
            Self::Baseline { column } => format!("Baseline of {column}"),
            Self::Cumulative { column } => format!("Cumulative integral of {column}"),
            Self::Normalized { column } => format!("Normalized integral of {column}"),
            Self::Fit { name } => format!("{name} (fit)"),
            Self::Residuals { name } => format!("{name} (resid.)"),
            Self::Peak { index, column } => format!("Peak {index} of {column}"),
            Self::Background { column } => format!("Background of {column}"),
            Self::PeaksSum { column } => format!("Sum of peaks of {column}"),
            Self::Deconvolved { name } => format!("{name} (deconvolved)"),
            Self::Smoothed { name } => format!("{name} (smoothed)"),
            Self::Source { name } => name.clone(),
        }
    }
}

/// The unit of an output column: **always that of the source** for the
/// eleven known keys, empty for an unknown key.
pub fn output_unit(tree: &Tree, series: &SeriesRef, key: &str) -> String {
    match key {
        "corrected" | "baseline" | "cumulative" | "normalized" | "fit" | "residuals" | "peak"
        | "background" | "peaks_sum" | "deconvolved" | "smoothed" => tree
            .node(series.y)
            .and_then(|n| n.as_column())
            .map(|c| c.meta().unit.clone())
            .unwrap_or_default(),
        _ => String::new(),
    }
}

/// The raw English name of an operation family: the comment of the output
/// columns written by `pyst` and the `Display` of history labels. The
/// application's `analysis-kind-*` catalog is its translation.
pub fn family_english(kind: OperationKind) -> &'static str {
    match kind {
        OperationKind::Baseline => "Correction",
        OperationKind::Correct => "Reference",
        OperationKind::Integrate => "Integration",
        OperationKind::Fit => "Fit",
        OperationKind::Peaks => "Peak Decomposition",
        OperationKind::Reconvolution => "Reconvolution",
        OperationKind::DirectDeconvolution => "Direct Deconvolution",
        OperationKind::Smooth => "Smoothing",
    }
}

/// The two output settings of a peak decomposition that are not in the
/// [`Operation`] (nor in the file): the "one peak + background" columns and the
/// "sum of peaks" column. The window's checkboxes and the options of
/// `pyst.peaks` carry them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PeakOutputs {
    /// One column per peak and the background column.
    pub peak_columns: bool,
    /// The column of the sum of peaks.
    pub sum_column: bool,
}

impl Default for PeakOutputs {
    /// The default values of the window: everything is written.
    fn default() -> Self {
        Self {
            peak_columns: true,
            sum_column: true,
        }
    }
}

/// Is output `key` of `op` to be written? A single list for the analysis
/// window and `pyst`: the helper columns (baseline, integrals,
/// residuals) follow the operation's option; those of a peak decomposition
/// follow `peaks`. Every other output is written.
pub fn wants_output(op: &Operation, key: &str, peaks: PeakOutputs) -> bool {
    match (op, key) {
        (Operation::Baseline { keep_baseline, .. }, "baseline") => *keep_baseline,
        (Operation::Integrate { spec }, "cumulative") => spec.cumulative,
        (Operation::Integrate { spec }, "normalized") => spec.normalize,
        (Operation::Fit { residuals, .. }, "residuals")
        | (Operation::Peaks { residuals, .. }, "residuals")
        | (Operation::Reconvolution { residuals, .. }, "residuals") => *residuals,
        (Operation::Peaks { .. }, "peak" | "background") => peaks.peak_columns,
        (Operation::Peaks { .. }, "peaks_sum") => peaks.sum_column,
        _ => true,
    }
}

#[cfg(test)]
mod output_name_tests {
    use super::*;
    use stokesforge_analysis::baseline::{BaselineMethod, BaselineOp};
    use stokesforge_analysis::integrate::{IntegrateSpec, Under};
    use stokesforge_analysis::peaks::{PeakBackground, PeakShape, PeaksSpec};

    /// A sheet with column "B" (long name "Intensity", unit
    /// "counts") and column "C" (no long name or unit).
    fn fixture() -> (Project, SeriesRef, SeriesRef) {
        let mut project = Project::new("P").unwrap();
        let book = project.add_workbook(project.root(), Some("Book")).unwrap();
        let sheet = project.tree().children(book).unwrap()[0];
        let b = project
            .add_column(sheet, None, Some("B"), Column::from_f64(&[1.0, 2.0]))
            .unwrap();
        let c = project
            .add_column(sheet, None, Some("C"), Column::from_f64(&[1.0, 2.0]))
            .unwrap();
        project
            .set_column_meta(
                b,
                ColumnMeta {
                    long_name: "Intensity".into(),
                    unit: "counts".into(),
                    ..ColumnMeta::default()
                },
            )
            .unwrap();
        let series = |y| SeriesRef {
            x: None,
            y,
            sigma: None,
        };
        (project, series(b), series(c))
    }

    fn name(tree: &Tree, kind: OperationKind, s: &SeriesRef, key: &str) -> OutputName {
        OutputName::of(tree, kind, s, key, None)
    }

    #[test]
    fn a_baseline_correction_is_named_after_the_long_name_and_the_short_one() {
        let (project, b, _) = fixture();
        let n = name(project.tree(), OperationKind::Baseline, &b, "corrected");
        assert_eq!(
            n,
            OutputName::BaselineCorrected {
                name: "Intensity".into(),
                column: "B".into()
            }
        );
        assert_eq!(n.english(), "Intensity (B-corr)");
        // Outside the Baseline family: the corrected of a correction.
        let n = name(project.tree(), OperationKind::Correct, &b, "corrected");
        assert_eq!(n.english(), "Intensity (corrected)");
    }

    #[test]
    fn without_a_long_name_the_short_name_stands_in() {
        let (project, _, c) = fixture();
        let n = name(project.tree(), OperationKind::Baseline, &c, "corrected");
        assert_eq!(
            n,
            OutputName::BaselineCorrected {
                name: "C".into(),
                column: "C".into()
            }
        );
        assert_eq!(n.english(), "C (C-corr)");
    }

    #[test]
    fn peaks_are_counted_from_one_and_named_after_the_short_name() {
        let (project, b, _) = fixture();
        let tree = project.tree();
        let peak = |index| OutputName::of(tree, OperationKind::Peaks, &b, "peak", index);
        assert_eq!(peak(Some(2)).english(), "Peak 2 of B");
        assert_eq!(peak(None).english(), "Peak 1 of B");
        let sum = name(tree, OperationKind::Peaks, &b, "peaks_sum");
        assert_eq!(sum.english(), "Sum of peaks of B");
    }

    #[test]
    fn every_known_key_has_its_english_form() {
        let (project, b, _) = fixture();
        let tree = project.tree();
        let of = |kind, key| name(tree, kind, &b, key).english();
        assert_eq!(of(OperationKind::Baseline, "baseline"), "Baseline of B");
        assert_eq!(
            of(OperationKind::Integrate, "cumulative"),
            "Cumulative integral of B"
        );
        assert_eq!(
            of(OperationKind::Integrate, "normalized"),
            "Normalized integral of B"
        );
        assert_eq!(of(OperationKind::Fit, "fit"), "Intensity (fit)");
        assert_eq!(of(OperationKind::Fit, "residuals"), "Intensity (resid.)");
        assert_eq!(of(OperationKind::Peaks, "background"), "Background of B");
        assert_eq!(
            of(OperationKind::Reconvolution, "deconvolved"),
            "Intensity (deconvolved)"
        );
    }

    #[test]
    fn an_unknown_key_falls_back_on_the_source_name() {
        let (project, b, _) = fixture();
        let n = name(project.tree(), OperationKind::Fit, &b, "something_new");
        assert_eq!(
            n,
            OutputName::Source {
                name: "Intensity".into()
            }
        );
        assert_eq!(n.english(), "Intensity");
    }

    #[test]
    fn the_unit_is_the_one_of_the_source_for_the_eleven_keys() {
        let (project, b, c) = fixture();
        let tree = project.tree();
        for key in [
            "corrected",
            "baseline",
            "cumulative",
            "normalized",
            "fit",
            "residuals",
            "peak",
            "background",
            "peaks_sum",
            "deconvolved",
            "smoothed",
        ] {
            assert_eq!(output_unit(tree, &b, key), "counts", "{key}");
            assert_eq!(output_unit(tree, &c, key), "", "{key} without unit");
        }
        assert_eq!(output_unit(tree, &b, "something_new"), "");
    }

    #[test]
    fn each_family_has_its_english_title() {
        let titles: Vec<&str> = OperationKind::ALL.map(family_english).to_vec();
        assert_eq!(
            titles,
            [
                "Correction",
                "Reference",
                "Integration",
                "Fit",
                "Peak Decomposition",
                "Reconvolution",
                "Direct Deconvolution",
                "Smoothing"
            ]
        );
    }

    fn peaks_op() -> Operation {
        Operation::Peaks {
            spec: PeaksSpec {
                shape: PeakShape::Gaussian,
                background: PeakBackground::Constant,
                seeds: Vec::new(),
                link_widths: false,
                center_window: None,
            },
            auto: None,
            range: None,
            residuals: false,
        }
    }

    #[test]
    fn the_peak_options_rule_the_peak_columns_and_the_sum() {
        let op = peaks_op();
        for peak_columns in [false, true] {
            for sum_column in [false, true] {
                let options = PeakOutputs {
                    peak_columns,
                    sum_column,
                };
                assert_eq!(wants_output(&op, "peak", options), peak_columns);
                assert_eq!(wants_output(&op, "background", options), peak_columns);
                assert_eq!(wants_output(&op, "peaks_sum", options), sum_column);
                // What the options do not set stays written or follows the operation.
                assert!(wants_output(&op, "fit", options));
                assert!(!wants_output(&op, "residuals", options));
            }
        }
        assert!(PeakOutputs::default().peak_columns);
        assert!(PeakOutputs::default().sum_column);
    }

    #[test]
    fn the_helper_columns_follow_the_option_of_their_operation() {
        let options = PeakOutputs::default();
        let baseline = |keep_baseline| Operation::Baseline {
            method: BaselineMethod::Constant { point: (0.0, 0.0) },
            op: BaselineOp::Subtract,
            keep_baseline,
        };
        assert!(wants_output(&baseline(true), "baseline", options));
        assert!(!wants_output(&baseline(false), "baseline", options));
        assert!(wants_output(&baseline(false), "corrected", options));
        let integrate = |cumulative, normalize| Operation::Integrate {
            spec: IntegrateSpec {
                ranges: Vec::new(),
                under: Under::Zero,
                cumulative,
                normalize,
            },
        };
        let op = integrate(true, false);
        assert!(wants_output(&op, "cumulative", options));
        assert!(!wants_output(&op, "normalized", options));
    }
}

#[cfg(test)]
mod inputs_digest_tests {
    use super::*;
    use stokesforge_analysis::integrate::{IntegrateSpec, Under};

    fn calc_on(y: NodeId) -> Calculation {
        Calculation {
            operation: Operation::Integrate {
                spec: IntegrateSpec {
                    ranges: Vec::new(),
                    under: Under::Zero,
                    cumulative: false,
                    normalize: false,
                },
            },
            series: vec![SeriesRef {
                x: None,
                y,
                sigma: None,
            }],
            aux: None,
            outputs: Vec::new(),
            results: Vec::new(),
            keep_results: false,
            auto_recalc: false,
            low_r2_dialog: true,
            stale: None,
        }
    }

    fn sheet_with(columns: &[Column]) -> (Project, Vec<NodeId>) {
        let mut project = Project::new("P").unwrap();
        let book = project.add_workbook(project.root(), Some("Book")).unwrap();
        let sheet = project.tree().children(book).unwrap()[0];
        let ids = columns
            .iter()
            .enumerate()
            .map(|(i, c)| {
                project
                    .add_column(sheet, None, Some(&format!("C{i}")), c.clone())
                    .unwrap()
            })
            .collect();
        (project, ids)
    }

    const NUMERIC_DIGEST: u64 = 14_944_315_663_721_976_880;
    const MIXED_DIGEST: u64 = 16_594_512_645_915_888_955;

    fn digest(columns: &[Column]) -> Option<u64> {
        let (project, ids) = sheet_with(columns);
        calc_on(ids[0]).inputs_digest(project.tree())
    }

    /// The fingerprint is part of the `.stf` format (S3, #323): these values are
    /// written into files, they never change. A change here would make stale on
    /// reread all the calculations of files already written.
    #[test]
    fn the_digest_is_frozen_on_known_inputs() {
        let numeric = Column::from_f64(&[1.5, f64::NAN, -0.0, 0.0]);
        assert_eq!(digest(&[numeric]), Some(NUMERIC_DIGEST));
        let mixed = Column::from_mixed(&[
            MixedCell::Empty,
            MixedCell::Number(2.5),
            MixedCell::Text("é".into()),
        ]);
        assert_eq!(digest(&[mixed]), Some(MIXED_DIGEST));
    }

    #[test]
    fn the_same_content_under_two_ids_gives_the_same_digest() {
        let c = Column::from_f64(&[1.0, 2.0, f64::NAN]);
        // Two sheets, hence two sets of identifiers: only the content counts.
        let (mut project, ids) = sheet_with(std::slice::from_ref(&c));
        let extra = project.tree().children(project.root()).unwrap()[0];
        let sheet = project.tree().children(extra).unwrap()[0];
        let other = project
            .add_column(sheet, None, Some("Z"), c.clone())
            .unwrap();
        let a = calc_on(ids[0]).inputs_digest(project.tree());
        let b = calc_on(other).inputs_digest(project.tree());
        assert!(a.is_some());
        assert_eq!(a, b);
    }

    #[test]
    fn a_changed_cell_or_type_changes_the_digest() {
        let base = digest(&[Column::from_f64(&[1.0, 2.0, 3.0])]);
        assert_ne!(base, digest(&[Column::from_f64(&[1.0, 2.5, 3.0])]));
        assert_ne!(base, digest(&[Column::from_f64(&[1.0, 2.0])]));
        let mixed = Column::from_mixed(&[
            MixedCell::Number(1.0),
            MixedCell::Number(2.0),
            MixedCell::Number(3.0),
        ]);
        assert_ne!(base, digest(std::slice::from_ref(&mixed)));
        let text = Column::from_mixed(&[
            MixedCell::Number(1.0),
            MixedCell::Number(2.0),
            MixedCell::Text("3".into()),
        ]);
        assert_ne!(digest(&[mixed]), digest(&[text]));
        // The binary pattern counts: -0.0 is not 0.0.
        assert_ne!(
            digest(&[Column::from_f64(&[0.0])]),
            digest(&[Column::from_f64(&[-0.0])])
        );
    }

    #[test]
    fn a_missing_column_gives_no_digest() {
        let (project, _) = sheet_with(&[Column::from_f64(&[1.0])]);
        assert_eq!(
            calc_on(NodeId::from_raw(9999)).inputs_digest(project.tree()),
            None
        );
    }
}

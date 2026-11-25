//! Origin column formulas in an import (#556): which formula columns can be
//! set live, which keep Origin's values.
//!
//! Three pure steps, all before anything is created:
//!
//! 1. [`resolve_origin_formula`] translates the text of one formula column
//!    ([`crate::translate_origin_formula`]) and resolves every reference **by
//!    Origin short name**, in the referenced sheet or book of the **same
//!    file** (never by position letter). The chooser uses it once per file to
//!    warn about a formula that reads an unticked column.
//! 2. [`plan_origin_formulas`] maps the references onto the columns of an
//!    import plan (the shared-X map included) and gives each formula column a
//!    [`FormulaVerdict`].
//! 3. [`judge_origin_formulas`] builds a scratch [`Project`] holding the planned
//!    sheets with **Origin's values** only, computes each translated formula
//!    there with the pure functions of the model and compares the result with
//!    Origin's values: [`Judgement::Equal`], [`Judgement::Stale`] (with one
//!    example row) or [`Judgement::Refused`] (a loop, a column not where it
//!    must be, a formula the language refuses). A formula that reads another
//!    formula column is judged on Origin's values for that column.
//!
//! [`decide_origin_formulas`] then says which formulas are set and which
//! columns keep their values (they get [`with_formula_note`]), and
//! [`build_column_formula`] turns the pieces of a set formula into the stored
//! [`ColumnFormula`] once the real columns exist. The application, the CLI and
//! the scripts all go through this one path.

use std::collections::{BTreeMap, BTreeSet};

use stokesforge_core::{
    evaluate_formula_with_x, Column, ColumnFormula, FormulaPart, NodeId, Project, RemoteRef,
};
use stokesforge_import::origin::OriginProject;

use crate::origin::{make_origin_column_in, OriginColumnKey, OriginLayoutPlan, OriginSource};
use crate::origin_formula::{
    translate_origin_formula, FormulaPiece, FormulaReference, OriginFormula, UntranslatableReason,
};

/// Relative tolerance of the comparison with Origin's values.
const VALUE_TOLERANCE: f64 = 1e-9;
/// Absolute floor, as a share of the largest magnitude of the column.
const FLOOR_OF_COLUMN: f64 = 1e-12;

/// A column of a file by position: book, sheet, column.
pub type OriginColumnPos = (usize, usize, usize);

/// A piece of a formula whose references are resolved in one Origin file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolvedPiece {
    /// Text of the formula language.
    Text(String),
    /// The column read, and whether the formula text prefixes its sheet.
    Column { at: OriginColumnPos, prefixed: bool },
}

/// What [`resolve_origin_formula`] says of a formula column.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolvedFormula {
    /// Translated, every reference found in the file.
    Pieces(Vec<ResolvedPiece>),
    /// Refused by the translator (`Some`), or a reference names no column of
    /// the file (`None`).
    Untranslatable(Option<UntranslatableReason>),
}

fn find_column(
    project: &OriginProject,
    own: (usize, usize),
    reference: &FormulaReference,
) -> Option<OriginColumnPos> {
    let book = match &reference.book {
        Some(name) => project
            .books
            .iter()
            .position(|book| book.short_name.eq_ignore_ascii_case(name))?,
        None => own.0,
    };
    let sheets = &project.books.get(book)?.sheets;
    let sheet = match (&reference.sheet, &reference.book) {
        (Some(name), _) => sheets
            .iter()
            .position(|sheet| sheet.name.eq_ignore_ascii_case(name))?,
        (None, None) => own.1,
        // `[Book]B`: no sheet is named, none is guessed.
        (None, Some(_)) => return None,
    };
    let columns = &sheets.get(sheet)?.columns;
    let column = columns
        .iter()
        .position(|column| column.short_name == reference.short_name)
        .or_else(|| {
            columns.iter().position(|column| {
                column
                    .short_name
                    .eq_ignore_ascii_case(&reference.short_name)
            })
        })?;
    Some((book, sheet, column))
}

/// Translates the formula of the column at `at` and resolves its references in
/// `project`. `None` when the column has no formula.
pub fn resolve_origin_formula(
    project: &OriginProject,
    at: OriginColumnPos,
) -> Option<ResolvedFormula> {
    let text = project
        .books
        .get(at.0)?
        .sheets
        .get(at.1)?
        .columns
        .get(at.2)?
        .formula
        .as_deref()?;
    Some(match translate_origin_formula(text) {
        OriginFormula::Untranslatable(reason) => ResolvedFormula::Untranslatable(Some(reason)),
        OriginFormula::Pieces(pieces) => {
            let mut resolved = Vec::with_capacity(pieces.len());
            for piece in pieces {
                match piece {
                    FormulaPiece::Text(text) => resolved.push(ResolvedPiece::Text(text)),
                    FormulaPiece::Reference(reference) => {
                        let Some(found) = find_column(project, (at.0, at.1), &reference) else {
                            return Some(ResolvedFormula::Untranslatable(None));
                        };
                        resolved.push(ResolvedPiece::Column {
                            at: found,
                            prefixed: !reference.same_sheet_as_first,
                        });
                    }
                }
            }
            ResolvedFormula::Pieces(resolved)
        }
    })
}

/// A piece of a planned formula: the column is an Origin column of the plan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlannedPiece {
    Text(String),
    /// The Origin column whose imported copy is read (after the shared-X map).
    Column {
        key: OriginColumnKey,
        prefixed: bool,
    },
}

/// What the plan says of one formula column.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FormulaVerdict {
    /// Translated; every column read is in the import.
    Translated(Vec<PlannedPiece>),
    /// Translated, but it reads a column that is not imported (its Origin short
    /// name).
    NotImportedInput(String),
    /// Not translated (`Some(reason)`), or its references do not name one
    /// column of the file (`None`).
    Untranslatable(Option<UntranslatableReason>),
}

/// A formula column of the plan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannedFormula {
    /// The formula as Origin wrote it.
    pub original: String,
    pub verdict: FormulaVerdict,
}

/// The formula columns of `plan`, each with its verdict. A column the shared-X
/// layout drops is not in the plan and has no entry.
pub fn plan_origin_formulas(
    sources: &[OriginSource<'_>],
    plan: &OriginLayoutPlan,
) -> BTreeMap<OriginColumnKey, PlannedFormula> {
    let placed: BTreeSet<OriginColumnKey> = plan
        .books
        .iter()
        .flat_map(|book| &book.sheets)
        .flat_map(|sheet| &sheet.columns)
        .map(|column| column.key())
        .collect();
    let mut out = BTreeMap::new();
    for column in plan
        .books
        .iter()
        .flat_map(|book| &book.sheets)
        .flat_map(|sheet| &sheet.columns)
    {
        let Some(source) = sources.get(column.file) else {
            continue;
        };
        let at = (column.book, column.sheet, column.column);
        let Some(resolved) = resolve_origin_formula(source.project, at) else {
            continue;
        };
        let original = source.project.books[at.0].sheets[at.1].columns[at.2]
            .formula
            .clone()
            .unwrap_or_default();
        let verdict = match resolved {
            ResolvedFormula::Untranslatable(reason) => FormulaVerdict::Untranslatable(reason),
            ResolvedFormula::Pieces(pieces) => {
                let mut planned = Vec::with_capacity(pieces.len());
                let mut missing = None;
                for piece in pieces {
                    match piece {
                        ResolvedPiece::Text(text) => planned.push(PlannedPiece::Text(text)),
                        ResolvedPiece::Column { at: read, prefixed } => {
                            let key = (column.file, read.0, read.1, read.2);
                            let key = plan.x_map.get(&key).copied().unwrap_or(key);
                            if placed.contains(&key) {
                                planned.push(PlannedPiece::Column { key, prefixed });
                            } else if missing.is_none() {
                                missing = Some(
                                    source.project.books[read.0].sheets[read.1].columns[read.2]
                                        .short_name
                                        .clone(),
                                );
                            }
                        }
                    }
                }
                match missing {
                    Some(name) => FormulaVerdict::NotImportedInput(name),
                    None => FormulaVerdict::Translated(planned),
                }
            }
        };
        out.insert(column.key(), PlannedFormula { original, verdict });
    }
    out
}

/// A cell shown as an example of a difference.
#[derive(Debug, Clone, PartialEq)]
pub enum ExampleCell {
    Empty,
    Number(f64),
    Text(String),
}

/// The first row where the formula and Origin differ.
#[derive(Debug, Clone, PartialEq)]
pub struct StaleExample {
    /// Zero-based row.
    pub row: usize,
    pub origin: ExampleCell,
    pub formula: ExampleCell,
}

/// What the scratch computation says of a translated formula.
#[derive(Debug, Clone, PartialEq)]
pub enum Judgement {
    /// The formula gives Origin's values.
    Equal,
    /// It gives other values.
    Stale(StaleExample),
    /// It cannot be set: a loop or a self reference, a column that is not where
    /// it must be, a text the language refuses.
    Refused,
}

/// Who is who in the scratch project.
struct Scratch {
    project: Project,
    /// Origin column to (column, sheet) of the scratch project.
    at: BTreeMap<OriginColumnKey, (NodeId, NodeId)>,
}

fn build_scratch(sources: &[OriginSource<'_>], plan: &OriginLayoutPlan) -> Option<Scratch> {
    let mut project = Project::new("Scratch").ok()?;
    let root = project.root();
    let mut at = BTreeMap::new();
    for book_plan in &plan.books {
        let book = project.add_empty_workbook(root, None).ok()?;
        for sheet_plan in &book_plan.sheets {
            let sheet = project.add_worksheet(book, None).ok()?;
            for planned in &sheet_plan.columns {
                let Some(origin_book) = sources
                    .get(planned.file)
                    .and_then(|source| source.project.books.get(planned.book))
                else {
                    continue;
                };
                let Some(column) = origin_book
                    .sheets
                    .get(planned.sheet)
                    .and_then(|sheet| sheet.columns.get(planned.column))
                else {
                    continue;
                };
                let built = make_origin_column_in(origin_book, column, None);
                if let Ok(id) = project.add_column(sheet, None, None, built) {
                    at.insert(planned.key(), (id, sheet));
                }
            }
        }
    }
    Some(Scratch { project, at })
}

/// Builds the stored formula of a translated one: each column read is found by
/// `locate` (column and sheet), a column of another sheet than `own_sheet` is a
/// remote reference. `None` when a column cannot be located.
pub fn build_column_formula(
    pieces: &[PlannedPiece],
    own_sheet: NodeId,
    locate: &dyn Fn(OriginColumnKey) -> Option<(NodeId, NodeId)>,
) -> Option<ColumnFormula> {
    let mut parts = Vec::with_capacity(pieces.len());
    let mut remote = BTreeMap::new();
    for (index, piece) in pieces.iter().enumerate() {
        match piece {
            PlannedPiece::Text(text) => parts.push(FormulaPart::Text(text.clone())),
            PlannedPiece::Column { key, prefixed } => {
                let (column, sheet) = locate(*key)?;
                if sheet != own_sheet {
                    remote.insert(
                        index,
                        RemoteRef {
                            sheet,
                            prefixed: *prefixed,
                        },
                    );
                }
                parts.push(FormulaPart::Column(column));
            }
        }
    }
    Some(ColumnFormula::from_parts_with_remote(parts, remote))
}

/// Equal values: both empty, or within a relative `1e-9`, or within `floor`
/// (an absolute floor of the column, see [`first_difference`]).
fn same_value(a: f64, b: f64, floor: f64) -> bool {
    let gap = (a - b).abs();
    (a.is_nan() && b.is_nan())
        || a == b
        || gap <= VALUE_TOLERANCE * a.abs().max(b.abs())
        || gap <= floor
}

/// First row where the computed values differ from Origin's column. Trailing
/// empty cells are ignored on both sides; a text cell of Origin always differs.
/// Two numbers are equal within a relative `1e-9`, or within `1e-12 * M`, `M`
/// being the largest finite magnitude among Origin's values of the column (no
/// floor when there is none): float noise next to zero (`9e-20` against `0` in
/// a column that reaches `0.5`) is not a difference.
fn first_difference(
    origin: &stokesforge_import::origin::OriginColumn,
    computed: &[f64],
) -> Option<StaleExample> {
    let origin_len = (0..origin.values.len())
        .rev()
        .find(|&row| !origin.values[row].is_nan() || origin.text(row).is_some())
        .map_or(0, |row| row + 1);
    let computed_len = computed
        .iter()
        .rposition(|value| !value.is_nan())
        .map_or(0, |row| row + 1);
    let floor = FLOOR_OF_COLUMN
        * origin
            .values
            .iter()
            .filter(|value| value.is_finite())
            .fold(0.0f64, |m, value| m.max(value.abs()));
    for row in 0..origin_len.max(computed_len) {
        let got = computed.get(row).copied().unwrap_or(f64::NAN);
        let cell = |value: f64| {
            if value.is_nan() {
                ExampleCell::Empty
            } else {
                ExampleCell::Number(value)
            }
        };
        if let Some(text) = origin.text(row) {
            return Some(StaleExample {
                row,
                origin: ExampleCell::Text(text.to_string()),
                formula: cell(got),
            });
        }
        let expected = origin.values.get(row).copied().unwrap_or(f64::NAN);
        if !same_value(expected, got, floor) {
            return Some(StaleExample {
                row,
                origin: cell(expected),
                formula: cell(got),
            });
        }
    }
    None
}

/// Computes every translated formula of `formulas` in a scratch project of
/// `plan` and compares it with Origin's values.
pub fn judge_origin_formulas(
    sources: &[OriginSource<'_>],
    plan: &OriginLayoutPlan,
    formulas: &BTreeMap<OriginColumnKey, PlannedFormula>,
) -> BTreeMap<OriginColumnKey, Judgement> {
    let translated: Vec<(OriginColumnKey, &Vec<PlannedPiece>)> = formulas
        .iter()
        .filter_map(|(key, formula)| match &formula.verdict {
            FormulaVerdict::Translated(pieces) => Some((*key, pieces)),
            _ => None,
        })
        .collect();
    let mut out = BTreeMap::new();
    if translated.is_empty() {
        return out;
    }
    let Some(mut scratch) = build_scratch(sources, plan) else {
        return translated
            .into_iter()
            .map(|(key, _)| (key, Judgement::Refused))
            .collect();
    };
    // Every formula is set in the scratch project first: the loop check and
    // "ordinary column" read the stored formulas of the others.
    let mut built: Vec<(OriginColumnKey, NodeId, ColumnFormula)> = Vec::new();
    for (key, pieces) in &translated {
        let Some(&(column, sheet)) = scratch.at.get(key) else {
            out.insert(*key, Judgement::Refused);
            continue;
        };
        let locate = |k: OriginColumnKey| scratch.at.get(&k).copied();
        let Some(formula) = build_column_formula(pieces, sheet, &locate) else {
            out.insert(*key, Judgement::Refused);
            continue;
        };
        if formula.compile().is_err() {
            out.insert(*key, Judgement::Refused);
            continue;
        }
        built.push((*key, column, formula));
    }
    for (_, column, formula) in &built {
        let Some(mut meta) = scratch
            .project
            .tree()
            .node(*column)
            .and_then(|node| node.as_column())
            .map(|column| column.meta().clone())
        else {
            continue;
        };
        meta.formula = Some(formula.clone());
        let _ = scratch.project.set_column_meta(*column, meta);
    }
    let tree = scratch.project.tree();
    let ordinary_len = |sheet: NodeId| -> usize {
        tree.node(sheet)
            .map(|node| node.children().to_vec())
            .unwrap_or_default()
            .into_iter()
            .filter_map(|id| tree.node(id).and_then(|node| node.as_column()))
            .filter(|column| column.meta().formula.is_none())
            .map(Column::len)
            .max()
            .unwrap_or(0)
    };
    for (key, column, formula) in &built {
        if tree.formula_loop(*column, formula) {
            out.insert(*key, Judgement::Refused);
            continue;
        }
        let Ok(compiled) = formula.compile() else {
            out.insert(*key, Judgement::Refused);
            continue;
        };
        let inputs = tree.formula_inputs(*column, formula, &compiled);
        if !inputs.all_present() {
            out.insert(*key, Judgement::Refused);
            continue;
        }
        let (_, own_sheet) = scratch.at[key];
        let remote = formula.remote_sheets();
        let row_wise_remotes = compiled.row_wise_remotes();
        let referenced = compiled.referenced_columns();
        let length_of = |id: NodeId| {
            tree.node(id)
                .and_then(|node| node.as_column())
                .map(Column::len)
        };
        // Length of the result, as the recalculation takes it (#194, #556).
        let target_len = referenced
            .iter()
            .filter(|id| !remote.contains_key(id) || row_wise_remotes.contains(id))
            .filter_map(|&id| length_of(id))
            .max()
            .unwrap_or_else(|| ordinary_len(own_sheet));
        let names_own_column = referenced.iter().any(|id| !remote.contains_key(id));
        let target_len = if names_own_column {
            target_len
        } else {
            target_len.max(1)
        };
        let wanted: BTreeSet<NodeId> = referenced
            .iter()
            .copied()
            .chain(inputs.paired_x.values().copied())
            .collect();
        let values: Vec<_> = wanted
            .iter()
            .filter_map(|&id| {
                tree.node(id)
                    .and_then(|node| node.as_column())
                    .map(|column| (id, column.values().clone()))
            })
            .collect();
        let evaluation = evaluate_formula_with_x(&compiled, &values, &inputs.paired_x, target_len);
        if evaluation.error.is_some() {
            out.insert(*key, Judgement::Refused);
            continue;
        }
        let origin = &sources[key.0].project.books[key.1].sheets[key.2].columns[key.3];
        out.insert(
            *key,
            match first_difference(origin, &evaluation.values) {
                None => Judgement::Equal,
                Some(example) => Judgement::Stale(example),
            },
        );
    }
    out
}

/// The outcome of an import for its formula columns.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FormulaDecision {
    /// The formula columns whose formula is set.
    pub set: BTreeSet<OriginColumnKey>,
    /// The formula columns that keep Origin's values, with the original text
    /// (for [`with_formula_note`]).
    pub kept: BTreeMap<OriginColumnKey, String>,
}

/// Sets every formula that gives Origin's values, plus the stale ones in
/// `accepted`; every other formula column keeps its values.
pub fn decide_origin_formulas(
    formulas: &BTreeMap<OriginColumnKey, PlannedFormula>,
    judgements: &BTreeMap<OriginColumnKey, Judgement>,
    accepted: &BTreeSet<OriginColumnKey>,
) -> FormulaDecision {
    let mut decision = FormulaDecision::default();
    for (key, formula) in formulas {
        let set = match judgements.get(key) {
            Some(Judgement::Equal) => true,
            Some(Judgement::Stale(_)) => accepted.contains(key),
            _ => false,
        };
        if set {
            decision.set.insert(*key);
        } else {
            decision.kept.insert(*key, formula.original.clone());
        }
    }
    decision
}

/// The stale formula columns (values differ), in plan order of the keys.
pub fn stale_formulas(
    judgements: &BTreeMap<OriginColumnKey, Judgement>,
) -> Vec<(OriginColumnKey, StaleExample)> {
    judgements
        .iter()
        .filter_map(|(key, judgement)| match judgement {
            Judgement::Stale(example) => Some((*key, example.clone())),
            _ => None,
        })
        .collect()
}

/// The words that open the note a column keeps when its formula is not set.
pub const FORMULA_NOTE_PREFIX: &str = "Origin formula: ";

/// Appends `Origin formula: <original>` to the Comments of `column`
/// (after the existing comment, `; ` between them). A line break of the
/// formula text becomes a space: the Comments are one line.
pub fn with_formula_note(mut column: Column, original: &str) -> Column {
    let one_line = original.split(['\r', '\n']).collect::<Vec<_>>().join(" ");
    let note = format!("{FORMULA_NOTE_PREFIX}{}", one_line.trim());
    let mut meta = column.meta().clone();
    meta.comment = if meta.comment.trim().is_empty() {
        note
    } else {
        format!("{}; {note}", meta.comment)
    };
    column.set_meta(meta);
    column
}

/// The import of one Origin project that never asks (CLI, `pyst`): the plan,
/// the formula columns, and the decision (a formula is set only when it gives
/// Origin's values; any other formula column keeps its values and its note).
#[derive(Debug, Clone)]
pub struct OriginImportFormulas {
    pub plan: OriginLayoutPlan,
    pub formulas: BTreeMap<OriginColumnKey, PlannedFormula>,
    pub decision: FormulaDecision,
}

/// Plans, judges and decides the formulas of an import of `project` with
/// `selection`, one workbook per Origin workbook and one sheet per Origin sheet
/// ([`crate::plan_origin_import_layout`]).
pub fn plan_origin_import_formulas(
    project: &OriginProject,
    selection: &crate::origin::OriginSelection,
) -> OriginImportFormulas {
    let plan = crate::origin::plan_origin_import_layout(project, selection);
    let sources = [OriginSource {
        project,
        selection,
        file_stem: "",
    }];
    let formulas = plan_origin_formulas(&sources, &plan);
    let judgements = judge_origin_formulas(&sources, &plan, &formulas);
    let decision = decide_origin_formulas(&formulas, &judgements, &BTreeSet::new());
    OriginImportFormulas {
        plan,
        formulas,
        decision,
    }
}

/// The model column of an Origin column for an import: [`make_origin_column_in`]
/// with the note of a formula that is not set.
pub fn make_origin_import_column(
    project: &OriginProject,
    at: &crate::origin::OriginColumnRef,
    decision: &FormulaDecision,
) -> Option<Column> {
    let book = project.books.get(at.book)?;
    let column = book.sheets.get(at.sheet)?.columns.get(at.column)?;
    let built = make_origin_column_in(book, column, None);
    Some(match decision.kept.get(&at.key()) {
        Some(original) => with_formula_note(built, original),
        None => built,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use stokesforge_import::origin::{OriginColumn, OriginDesignation};

    fn column(values: &[f64]) -> OriginColumn {
        OriginColumn {
            short_name: "A".into(),
            designation: OriginDesignation::Y,
            long_name: String::new(),
            unit: String::new(),
            comment: String::new(),
            values: values.to_vec(),
            texts: Vec::new(),
            formula: None,
        }
    }

    #[test]
    fn noise_next_to_zero_in_a_column_that_reaches_half_is_equal() {
        let origin = column(&[0.5, 9e-20, 0.25]);
        assert!(first_difference(&origin, &[0.5, 0.0, 0.25]).is_none());
    }

    #[test]
    fn a_difference_of_1e_6_of_the_maximum_is_still_stale() {
        let origin = column(&[0.5, 0.0]);
        let found = first_difference(&origin, &[0.5, 0.5e-6]).expect("stale");
        assert_eq!(found.row, 1);
    }

    #[test]
    fn a_column_of_1e_20_values_differing_by_ten_percent_is_stale() {
        let origin = column(&[1e-20, 2e-20]);
        assert!(first_difference(&origin, &[1.1e-20, 2.2e-20]).is_some());
    }
}

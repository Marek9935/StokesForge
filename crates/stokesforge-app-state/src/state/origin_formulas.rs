//! The window `Formulas with Different Values` (#556): after Import is clicked
//! in the Origin chooser, and before anything is created, one row per formula
//! column whose formula gives other values than Origin's (the formula is
//! computed on Origin's own values, in a scratch project). The user ticks the
//! columns that must take the formula; the others keep Origin's values. All
//! boxes start unticked, except the answers of the last Import when it is
//! reopened. Cancel imports nothing and leaves the chooser open.

use std::collections::BTreeSet;

use stokesforge_core::number_format::format_exact;
use stokesforge_importer::{origin_book_name, ExampleCell, OriginColumnKey, StaleExample};

use super::origin_import::{FormulaPrep, OriginChooser};
use super::AppState;
use crate::tr;

/// One row of the window.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OriginFormulaRow {
    /// `Book · Sheet · Column = original Origin text`.
    pub label: String,
    /// `row 1: Origin 0.1215, formula 45.85`.
    pub example: String,
    /// The formula is set for this column.
    pub ticked: bool,
}

/// The open window: what the import will run with and the answers so far.
pub(super) struct OriginFormulaForm {
    prep: FormulaPrep,
    rows: Vec<(OriginColumnKey, String, String)>,
    ticked: BTreeSet<OriginColumnKey>,
}

fn cell_text(cell: &ExampleCell) -> String {
    match cell {
        ExampleCell::Empty => tr!("origin-formulas-empty"),
        ExampleCell::Number(value) => format_exact(*value),
        ExampleCell::Text(text) => format!("“{text}”"),
    }
}

impl OriginFormulaForm {
    pub(super) fn new(
        chooser: &OriginChooser,
        prep: FormulaPrep,
        stale: Vec<(OriginColumnKey, StaleExample)>,
    ) -> Self {
        let rows: Vec<(OriginColumnKey, String, String)> = stale
            .into_iter()
            .filter_map(|(key, example)| {
                let file = chooser.files.get(key.0)?;
                let book = file.project.books.get(key.1)?;
                let sheet = book.sheets.get(key.2)?;
                let column = sheet.columns.get(key.3)?;
                let original = prep.formulas.get(&key)?.original.clone();
                let label = tr!(
                    "origin-formulas-row",
                    book = origin_book_name(book),
                    sheet = sheet.name.as_str(),
                    column = column.short_name.as_str(),
                    formula = original.as_str()
                );
                let row = example.row + 1;
                let shown = tr!(
                    "origin-formulas-example",
                    row = row,
                    origin = cell_text(&example.origin),
                    formula = cell_text(&example.formula)
                );
                Some((key, label, shown))
            })
            .collect();
        let ticked = chooser
            .accepted_formulas
            .iter()
            .filter(|key| rows.iter().any(|(k, ..)| k == *key))
            .copied()
            .collect();
        Self { prep, rows, ticked }
    }
}

impl AppState {
    /// The window `Formulas with Different Values` is open.
    #[must_use]
    pub fn origin_formulas_open(&self) -> bool {
        self.origin_formulas.is_some()
    }

    /// Its rows, in the order of the plan.
    #[must_use]
    pub fn origin_formulas_rows(&self) -> Vec<OriginFormulaRow> {
        let Some(form) = &self.origin_formulas else {
            return Vec::new();
        };
        form.rows
            .iter()
            .map(|(key, label, example)| OriginFormulaRow {
                label: label.clone(),
                example: example.clone(),
                ticked: form.ticked.contains(key),
            })
            .collect()
    }

    pub(super) fn set_origin_formula_tick(&mut self, index: usize, ticked: bool) {
        let Some(form) = &mut self.origin_formulas else {
            return;
        };
        let Some(&(key, ..)) = form.rows.get(index) else {
            return;
        };
        if ticked {
            form.ticked.insert(key);
        } else {
            form.ticked.remove(&key);
        }
    }

    pub(super) fn check_all_origin_formulas(&mut self, ticked: bool) {
        let Some(form) = &mut self.origin_formulas else {
            return;
        };
        form.ticked = if ticked {
            form.rows.iter().map(|(key, ..)| *key).collect()
        } else {
            BTreeSet::new()
        };
    }

    /// Import: the ticked formulas are set, the others keep Origin's values.
    pub(super) fn confirm_origin_formulas(&mut self) {
        let Some(form) = self.origin_formulas.take() else {
            return;
        };
        let Some(chooser) = self.origin_import.clone() else {
            return;
        };
        self.run_origin_import(chooser, form.prep, form.ticked);
    }

    /// Cancel: nothing is imported, the chooser stays open.
    pub(super) fn cancel_origin_formulas(&mut self) {
        self.origin_formulas = None;
    }
}

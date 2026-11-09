//! The rare settings of the import screen, in the « Advanced » fold at the
//! bottom of the left panel: date order, characters removed at the start of
//! every data line, how a file with several blocks is split, the words read as
//! missing values, the prefix of comment lines, the data lines skipped. The
//! detector sets none of them by hand; the fold is closed by default.
//!
//! A setting that is not at its default value also reads as a **sentence** in
//! the list of the left panel ([`AppState::import_advanced_sentences`]), so
//! that nothing set there stays hidden.

use stokesforge_import::{BlockSplit, DateOrder, ImportRecipe};

use super::AppState;
use crate::tr;

/// One of the rare settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdvancedSetting {
    DateOrder,
    SkipChars,
    Blocks,
    MissingValues,
    CommentPrefix,
    SkipLines,
}

/// Every rare setting, in the order of the fold.
pub const ADVANCED_SETTINGS: [AdvancedSetting; 6] = [
    AdvancedSetting::DateOrder,
    AdvancedSetting::SkipChars,
    AdvancedSetting::Blocks,
    AdvancedSetting::MissingValues,
    AdvancedSetting::CommentPrefix,
    AdvancedSetting::SkipLines,
];

/// A new value for a rare setting.
#[derive(Debug, Clone, PartialEq)]
pub enum AdvancedChange {
    DateOrder(DateOrder),
    SkipChars(usize),
    Blocks(BlockSplit),
    /// The words read as missing values, separated by commas.
    MissingValues(String),
    CommentPrefix(String),
    SkipLines(usize),
}

/// A row of the fold: the setting, its label and its value as the field shows
/// it.
#[derive(Debug, Clone, PartialEq)]
pub struct AdvancedRow {
    pub setting: AdvancedSetting,
    pub label: String,
    /// The value as text (empty for « none »).
    pub value: String,
    /// The value is not the default one: it also has a sentence.
    pub set: bool,
}

impl AdvancedSetting {
    pub fn label(self) -> String {
        match self {
            AdvancedSetting::DateOrder => tr!("import-advanced-date-order"),
            AdvancedSetting::SkipChars => tr!("import-advanced-skip-chars"),
            AdvancedSetting::Blocks => tr!("import-advanced-blocks"),
            AdvancedSetting::MissingValues => tr!("import-advanced-missing"),
            AdvancedSetting::CommentPrefix => tr!("import-advanced-comment-prefix"),
            AdvancedSetting::SkipLines => tr!("import-advanced-skip-lines"),
        }
    }
}

/// The value of a setting as text, and the sentence it gives when it is set.
fn describe(recipe: &ImportRecipe, setting: AdvancedSetting) -> (String, Option<String>) {
    match setting {
        AdvancedSetting::DateOrder => match recipe.date_order {
            DateOrder::DayFirst => (tr!("import-advanced-date-day-first"), None),
            DateOrder::MonthFirst => (
                tr!("import-advanced-date-month-first"),
                Some(tr!("import-advanced-sentence-date-month-first")),
            ),
        },
        AdvancedSetting::SkipChars => (
            recipe.skip_chars.to_string(),
            (recipe.skip_chars > 0).then(|| {
                tr!(
                    "import-advanced-sentence-skip-chars",
                    count = recipe.skip_chars
                )
            }),
        ),
        AdvancedSetting::Blocks => match &recipe.block_split {
            BlockSplit::None => (tr!("import-advanced-blocks-single"), None),
            BlockSplit::BeforeLine(text) => (
                text.clone(),
                Some(tr!(
                    "import-advanced-sentence-blocks-line",
                    text = text.clone()
                )),
            ),
            BlockSplit::BlankLines => (
                tr!("import-advanced-blocks-blank"),
                Some(tr!("import-advanced-sentence-blocks-blank")),
            ),
        },
        AdvancedSetting::MissingValues => {
            let tokens = recipe.missing_tokens.join(", ");
            let sentence = (!tokens.is_empty())
                .then(|| tr!("import-advanced-sentence-missing", tokens = tokens.clone()));
            (tokens, sentence)
        }
        AdvancedSetting::CommentPrefix => (
            recipe.comment_prefix.clone(),
            (!recipe.comment_prefix.is_empty()).then(|| {
                tr!(
                    "import-advanced-sentence-comment",
                    prefix = recipe.comment_prefix.clone()
                )
            }),
        ),
        AdvancedSetting::SkipLines => (
            recipe.skip_data_rows.to_string(),
            (recipe.skip_data_rows > 0).then(|| {
                tr!(
                    "import-advanced-sentence-skip-lines",
                    count = recipe.skip_data_rows
                )
            }),
        ),
    }
}

/// Applies a change to a recipe.
pub fn apply_advanced(recipe: &mut ImportRecipe, change: AdvancedChange) {
    match change {
        AdvancedChange::DateOrder(order) => recipe.date_order = order,
        AdvancedChange::SkipChars(count) => recipe.skip_chars = count,
        AdvancedChange::Blocks(split) => recipe.block_split = split,
        AdvancedChange::MissingValues(text) => {
            recipe.missing_tokens = text
                .split(',')
                .map(|token| token.trim().to_string())
                .filter(|token| !token.is_empty())
                .collect();
        }
        AdvancedChange::CommentPrefix(prefix) => recipe.comment_prefix = prefix.trim().to_string(),
        AdvancedChange::SkipLines(count) => recipe.skip_data_rows = count,
    }
}

impl AppState {
    /// The rows of the « Advanced » fold, with the recipe being edited.
    pub fn import_advanced(&self) -> Vec<AdvancedRow> {
        let Some(import) = self.import_state.as_ref() else {
            return Vec::new();
        };
        ADVANCED_SETTINGS
            .into_iter()
            .map(|setting| {
                let (value, sentence) = describe(&import.recipe, setting);
                AdvancedRow {
                    setting,
                    label: setting.label(),
                    value,
                    set: sentence.is_some(),
                }
            })
            .collect()
    }

    /// One sentence per rare setting that is not at its default value, for the
    /// list of the left panel.
    pub fn import_advanced_sentences(&self) -> Vec<String> {
        let Some(import) = self.import_state.as_ref() else {
            return Vec::new();
        };
        ADVANCED_SETTINGS
            .into_iter()
            .filter_map(|setting| describe(&import.recipe, setting).1)
            .collect()
    }

    /// Changes a rare setting of the recipe being edited.
    pub fn import_set_advanced(&mut self, change: AdvancedChange) {
        if let Some(import) = self.import_state.as_mut() {
            apply_advanced(&mut import.recipe, change);
        }
    }
}

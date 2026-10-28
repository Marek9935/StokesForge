//! From the result of [`stokesforge_import`] to the **model columns**: which files
//! (and which blocks) go on which worksheets ([`plan_import`]), and how each
//! column is built ([`make_import_column`]: numeric, date-time or mixed
//! type, long name, unit, role, file metadata, import
//! source).
//!
//! This crate **writes into no project**: it returns `Column`s and plans.
//! Each consumer decides how to write — the application through undoable
//! commands in an "Import" macro, the CLI directly into a fresh
//! `Project`, the `pyst` scripts through the command bus of their session.
//! This is what lets a single implementation of the conversion serve everywhere
//! (`stokesforge-import` itself stays independent of the model).

use std::path::{Path, PathBuf};

use stokesforge_core::{
    Column, ColumnMeta, ColumnRole, ColumnType, ImportSource, MixedCell, ValueBlock,
};
use stokesforge_import::{
    ColumnKind, ColumnRoleChoice, FilesLayout, ImportError, ImportRecipe, ParsedTable,
};

/// Why a file could not be read or parsed. Structured, so that the
/// application puts it into a sentence in the interface language; the English
/// [`Display`](std::fmt::Display) serves the CLI and the scripts.
#[derive(Debug, Clone, PartialEq)]
pub struct ImportFailure {
    /// The file concerned.
    pub path: PathBuf,
    /// The cause.
    pub cause: ImportFailureCause,
}

/// The cause of an [`ImportFailure`].
#[derive(Debug, Clone, PartialEq)]
pub enum ImportFailureCause {
    /// The file could not be read: the system message (`io::Error`
    /// is not `Clone`).
    Read(String),
    /// The file was read but not parsed.
    Parse(ImportError),
}

impl std::fmt::Display for ImportFailureCause {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Read(message) => f.write_str(message),
            Self::Parse(error) => error.fmt(f),
        }
    }
}

impl std::fmt::Display for ImportFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.path.display(), self.cause)
    }
}

impl std::error::Error for ImportFailure {}

/// A file (or a block of a file) to import: its path, the recipe that
/// reads it, and the parse result (or its error).
#[derive(Debug, Clone)]
pub struct ImportItem {
    pub path: PathBuf,
    pub recipe: ImportRecipe,
    pub table: Result<ParsedTable, ImportFailure>,
    /// Rank of the block when the file contains several
    /// (`ImportRecipe::block_split`); `None` for a single-block file.
    pub block: Option<usize>,
}

/// Reads a file and returns **one item per data block** (just one for an
/// ordinary file); a read or parse error gives one item in
/// error.
pub fn import_items(path: &Path, recipe: &ImportRecipe) -> Vec<ImportItem> {
    let failed = |cause: ImportFailureCause| {
        vec![ImportItem {
            path: path.to_path_buf(),
            recipe: recipe.clone(),
            table: Err(ImportFailure {
                path: path.to_path_buf(),
                cause,
            }),
            block: None,
        }]
    };
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(err) => return failed(ImportFailureCause::Read(err.to_string())),
    };
    match stokesforge_import::parse_blocks(&bytes, recipe) {
        Ok(tables) => {
            let several = tables.len() > 1;
            tables
                .into_iter()
                .enumerate()
                .map(|(index, table)| ImportItem {
                    path: path.to_path_buf(),
                    recipe: recipe.clone(),
                    table: Ok(table),
                    block: several.then_some(index),
                })
                .collect()
        }
        Err(err) => failed(ImportFailureCause::Parse(err)),
    }
}

/// A column to create: column `column` of item `item`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlannedColumn {
    pub item: usize,
    pub column: usize,
}

/// A worksheet to fill: its name, its columns, the items that
/// contribute to it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportPlan {
    pub name: String,
    pub columns: Vec<PlannedColumn>,
    pub items: Vec<usize>,
}

/// Distributes the columns of the readable items into worksheets according to `layout`: one
/// worksheet per item, or all side by side, or all sharing the
/// first one's X. The "Ignore" columns are excluded everywhere.
pub fn plan_import(items: &[ImportItem], layout: FilesLayout) -> Vec<ImportPlan> {
    // Worksheet name: the file, followed by the block rank if there are several.
    let stem = |item: usize| {
        let base = items[item]
            .path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("file");
        match items[item].block {
            Some(block) => format!("{base}_{}", block + 1),
            None => base.to_string(),
        }
    };
    let kept = |item: usize| -> Vec<usize> {
        let ImportItem { recipe, table, .. } = &items[item];
        let count = table.as_ref().map_or(0, |t| t.columns.len());
        (0..count)
            .filter(|&c| recipe.column_role(c) != ColumnRoleChoice::Ignore)
            .collect()
    };
    let readable: Vec<usize> = (0..items.len())
        .filter(|&i| items[i].table.is_ok())
        .collect();
    match layout {
        FilesLayout::SheetPerFile => readable
            .iter()
            .map(|&item| ImportPlan {
                name: stem(item),
                columns: kept(item)
                    .into_iter()
                    .map(|column| PlannedColumn { item, column })
                    .collect(),
                items: vec![item],
            })
            .collect(),
        FilesLayout::SideBySide | FilesLayout::SharedX => {
            let Some(&first) = readable.first() else {
                return Vec::new();
            };
            let mut columns = Vec::new();
            for &item in &readable {
                for column in kept(item) {
                    // Shared X: the first one's; the following ones bring none.
                    let is_x = items[item].recipe.column_role(column) == ColumnRoleChoice::X;
                    if layout == FilesLayout::SharedX && item != first && is_x {
                        continue;
                    }
                    columns.push(PlannedColumn { item, column });
                }
            }
            vec![ImportPlan {
                name: stem(first),
                columns,
                items: readable,
            }]
        }
    }
}

/// The values of a planned column, as a block ready to write: numbers, or
/// mixed cells as soon as a text is kept (a column forced to text
/// included).
pub fn import_block(items: &[ImportItem], planned: PlannedColumn) -> Option<ValueBlock> {
    let ImportItem { recipe, table, .. } = items.get(planned.item)?;
    let parsed = table.as_ref().ok()?.columns.get(planned.column)?;
    let forced_text = recipe.column_kind(planned.column) == ColumnKind::Text;
    Some(if forced_text || parsed.texts.iter().any(Option::is_some) {
        ValueBlock::Mixed(
            parsed
                .values
                .iter()
                .zip(&parsed.texts)
                .map(|(value, text)| match text {
                    Some(text) => MixedCell::Text(text.clone()),
                    None => MixedCell::from_f64(*value),
                })
                .collect(),
        )
    } else {
        ValueBlock::Float(parsed.values.clone())
    })
}

/// Builds the `planned` column: typed values (numeric, date-time,
/// mixed), long name, unit, comment, role, file metadata and import
/// source. `None` for an "Ignore" column. `prefix` empty cells are
/// added at the top (new column of an "append rows" import).
pub fn make_import_column(
    items: &[ImportItem],
    planned: PlannedColumn,
    prefix: usize,
) -> Option<Column> {
    let ImportItem {
        path,
        recipe,
        table,
        block: item_block,
    } = items.get(planned.item)?;
    let table = table.as_ref().ok()?;
    let index = planned.column;
    let parsed_column = table.columns.get(index)?;
    let file_stem = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("file");
    // Role chosen at the "Columns" step of the wizard: an "Ignore" column
    // is not imported.
    let role = match recipe.column_role(index) {
        ColumnRoleChoice::Ignore => return None,
        ColumnRoleChoice::X => ColumnRole::X,
        ColumnRoleChoice::XError => ColumnRole::XError,
        ColumnRoleChoice::YError => ColumnRole::YError,
        ColumnRoleChoice::Y | ColumnRoleChoice::Auto => ColumnRole::Y,
    };
    // A column with at least one cell carrying a kept text (`n/a`, an
    // instrument code…) becomes a mixed-type column rather than a numeric
    // column that would have flattened everything to `NaN`; a fully
    // numeric column keeps the `Numeric` path.
    let mut column = match import_block(items, planned)? {
        ValueBlock::Mixed(mut cells) => {
            if prefix > 0 {
                let mut padded = vec![MixedCell::Empty; prefix];
                padded.append(&mut cells);
                cells = padded;
            }
            Column::from_mixed(&cells)
        }
        ValueBlock::Float(mut values) => {
            if prefix > 0 {
                let mut padded = vec![f64::NAN; prefix];
                padded.append(&mut values);
                values = padded;
            }
            let mut numeric = Column::from_f64(&values);
            if parsed_column.datetime {
                // Dates and times: the same `f64`s (POSIX timestamps), typed
                // `DateTime` for display and axes.
                numeric.set_column_type(ColumnType::DateTime);
            }
            numeric
        }
    };
    // The long name prefers the `long_name` sub-header role, when the
    // recipe assigns one — otherwise the column names row, then "X" for
    // the first column, then the series name (file or header key).
    let long_name =
        parsed_column
            .long_name
            .clone()
            .unwrap_or_else(|| match (index, &parsed_column.name) {
                (_, Some(name)) => name.clone(),
                (0, None) => "X".to_string(),
                (_, None) => stokesforge_import::resolve_series_name(recipe, table, file_stem),
            });
    column.set_meta(ColumnMeta {
        long_name,
        unit: parsed_column.unit.clone().unwrap_or_default(),
        comment: parsed_column.comment.clone().unwrap_or_default(),
        x_prime: parsed_column.x_prime,
        // File metadata, copied onto each column: several files
        // in the same worksheet each keep their own.
        file_metadata: table
            .metadata()
            .iter()
            .map(|line| stokesforge_core::MetaLine {
                key: line.key.clone(),
                value: line.value.clone(),
            })
            .collect(),
        // By default the first imported column is the abscissa
        // (`ImportRecipe::column_role`).
        role,
        // Enough to refresh the column from its file.
        import_source: Some(ImportSource {
            path: path.display().to_string(),
            recipe_json: recipe.to_json(),
            column: index,
            block: item_block.unwrap_or(0),
        }),
        ..Default::default()
    });
    Some(column)
}

#[cfg(test)]
mod date_reader_agreement {
    /// #218: the core's date reader (grid, paste, script) and the import's
    /// are two copies; they read the same texts the same way.
    #[test]
    fn the_core_date_reader_agrees_with_the_import_one() {
        for text in [
            "2026-10-10",
            "2026-10-10 14:30",
            "2026-10-10T14:30:05",
            "2026-10-10 14:30:05,25",
            "10/10/2026 14:30",
            "10/11/2026",
            "25/11/24 15:17:22",
            "25.11.2024",
            "15:17:22",
            "01/13/2026",
            "31/04/2024",
            "2024-02-29",
            "1900-02-29",
            "abc",
            "1.5",
            "",
        ] {
            assert_eq!(
                stokesforge_core::datetime::parse_datetime(text).map(f64::to_bits),
                stokesforge_import::parse_datetime_for_agreement(text).map(f64::to_bits),
                "{text:?}"
            );
        }
    }
}

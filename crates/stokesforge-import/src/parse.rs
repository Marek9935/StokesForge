//! The parsing engine: applies an [`crate::ImportRecipe`] to raw bytes to
//! produce a [`ParsedTable`]. No unreadable token and no malformed line
//! interrupts the import. See invariant 10 ("a data defect degrades, it does
//! not crash"): unparseable values become `f64::NAN`, never an error.
//!
//! Only three cases fail the whole import: a file that is not text (binary;
//! see [`crate::decode`] for the accepted encodings), an empty file, or a
//! file in which no data line is found after the recipe is applied (header
//! marker not found, or nothing left after the header/footer).

/// A column as read from the file.
#[derive(Debug, Clone, PartialEq)]
pub struct ParsedColumn {
    /// Name read from the column header line, if `has_column_header_row` is
    /// set and this column had a non-empty entry there.
    pub name: Option<String>,
    /// One value per data line, in order. `f64::NAN` for a missing or
    /// unparseable value; never a blocking error for an individual unreadable
    /// token (invariant 10).
    pub values: Vec<f64>,
    /// The original token (trimmed) for each line where `values` holds a
    /// `NaN` **because the token was not a number**; `None` otherwise (the
    /// token was absent, empty, or parsed fine). Same length as `values`.
    /// Used downstream to tell a real missing value from text to keep
    /// (`stokesforge_core::ColumnType::Mixed`, the "mixed-type columns" work
    /// package). It is the first real consumer of that type: without this
    /// field, `1,2 / n/a / 1,5` lost the `n/a`.
    pub texts: Vec<Option<String>>,
    /// Long name extracted from the sub-header lines assigned to the "Long
    /// Names" role ([`crate::recipe::SubheaderRoles::long_name`]), if any.
    pub long_name: Option<String>,
    /// Unit extracted from the sub-header lines assigned to the "Units"
    /// role.
    pub unit: Option<String>,
    /// Comment extracted from the sub-header lines assigned to the "Comments"
    /// role.
    pub comment: Option<String>,
    /// x' value read from the line assigned to the `x_prime` role (#164),
    /// with the recipe's number format; `None` if the line is missing or the
    /// token is not a finite number.
    pub x_prime: Option<f64>,
    /// Column read as dates and times ([`crate::recipe::ColumnKind::DateTime`]):
    /// `values` then holds POSIX timestamps in seconds.
    pub datetime: bool,
}

/// The separator actually used for this file, once `Separator::Auto` (and
/// `Separator::Whitespace`) is resolved. `pub(crate)`: also useful to
/// [`crate::preview`] (live preview) and to the automatic detection of header
/// lines, not only to find the key/value pairs of the skipped header lines.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum EffectiveSeparator {
    Char(char),
    Whitespace,
    /// Fixed-width columns: the width of each one except the last.
    Fixed(Vec<usize>),
}

impl EffectiveSeparator {
    /// Splits a line into tokens with this separator. Never removes empty
    /// tokens for a character separator (a CSV `a;;c` does have 3 fields, the
    /// second empty); `Whitespace` absorbs them naturally; `Fixed` cuts at the
    /// given positions (in characters).
    pub(crate) fn tokenize<'a>(&self, line: &'a str) -> Vec<&'a str> {
        match self {
            EffectiveSeparator::Char(c) => line.split(*c).collect(),
            EffectiveSeparator::Whitespace => line.split_whitespace().collect(),
            EffectiveSeparator::Fixed(widths) => {
                let mut tokens = Vec::with_capacity(widths.len() + 1);
                let mut rest = line;
                for &width in widths {
                    let cut = rest
                        .char_indices()
                        .nth(width)
                        .map_or(rest.len(), |(byte, _)| byte);
                    tokens.push(&rest[..cut]);
                    rest = &rest[cut..];
                }
                tokens.push(rest);
                tokens
            }
        }
    }

    /// Splits a `KEY<separator>VALUE` line into `(key, value)` at the first
    /// occurrence of the separator. Not applicable to fixed-width columns: the
    /// line then has no key/value form.
    pub(crate) fn split_key_value<'a>(&self, line: &'a str) -> Option<(&'a str, &'a str)> {
        match self {
            EffectiveSeparator::Char(c) => line.split_once(*c),
            EffectiveSeparator::Whitespace => line.split_once(char::is_whitespace),
            EffectiveSeparator::Fixed(_) => line.split_once(['=', ':', ';', '\t']),
        }
    }
}

/// The result of a successful import: the columns read, plus any non-blocking
/// anomaly noticed along the way.
#[derive(Debug, Clone, PartialEq)]
pub struct ParsedTable {
    pub columns: Vec<ParsedColumn>,
    /// Non-blocking anomalies noticed while parsing (e.g. a point count
    /// announced in the header that does not match the number of lines
    /// actually read). Never filled in case of error: these are warnings on an
    /// import that succeeded.
    pub warnings: Vec<String>,
    header_lines: Vec<String>,
    footer_lines: Vec<String>,
    metadata: Vec<crate::metadata::MetadataLine>,
    separator: EffectiveSeparator,
}

impl ParsedTable {
    /// Number of data lines (0 if `columns` is empty).
    pub fn row_count(&self) -> usize {
        self.columns.first().map_or(0, |c| c.values.len())
    }

    /// Metadata to keep, according to the recipe (header, footer, exclusions);
    /// see [`crate::metadata`].
    pub fn metadata(&self) -> &[crate::metadata::MetadataLine] {
        &self.metadata
    }

    /// Lines that follow the end of the data (footer), as they are in the
    /// file. Never imported.
    pub fn footer_lines(&self) -> &[String] {
        &self.footer_lines
    }

    /// Looks, among the header lines skipped during parsing, for a line of
    /// the form `KEY<separator>VALUE` where KEY matches `key` (case
    /// insensitive, surrounding whitespace ignored). Returns VALUE (trimmed),
    /// or `None` if absent. `<separator>` is the one actually resolved for
    /// this file (useful even when the recipe asked for `Separator::Auto`).
    pub fn header_value(&self, key: &str) -> Option<String> {
        lookup_header_value(&self.header_lines, &self.separator, key)
    }
}

/// Looks for `key` in `header_lines`, in the form `KEY<separator>VALUE`.
fn lookup_header_value(
    header_lines: &[String],
    separator: &EffectiveSeparator,
    key: &str,
) -> Option<String> {
    let key_lower = key.trim().to_lowercase();
    header_lines.iter().find_map(|line| {
        let (k, v) = separator.split_key_value(line)?;
        if k.trim().to_lowercase() == key_lower {
            Some(v.trim().to_string())
        } else {
            None
        }
    })
}

/// The ways an import can fail entirely; everything else degrades rather than
/// failing (invariant 10).
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum ImportError {
    #[error("the file is not readable text (binary file)")]
    NotText,
    #[error("the file is empty")]
    Empty,
    #[error("no data line found after the header")]
    NoDataRows,
    /// "Whitespace" separator (chosen, or detected) with the space as
    /// thousands separator: `1 234,5` would be split into two columns.
    /// Rejected in the preview as in the import.
    #[error("whitespace column separator cannot be combined with a space as thousands separator")]
    WhitespaceWithSpaceThousands,
}

use crate::recipe::{DecimalStyle, ImportRecipe, NameSource, Separator};

/// Parses `bytes` according to `recipe`. `Err(ImportError::NotText)` if
/// `bytes` is not text ([`crate::decode::decode_with`]: the chosen or
/// detected encoding: UTF-8, UTF-16 with a mark, Windows-1252). Never panics
/// on a binary file.
///
/// The split into zones (header, data, footer...) comes from
/// [`crate::layout::analyze`], the same as the preview's.
///
/// A file with **several blocks** (`recipe.block_split`) returns its first
/// block here; [`parse_blocks`] returns them all.
pub fn parse(bytes: &[u8], recipe: &ImportRecipe) -> Result<ParsedTable, ImportError> {
    parse_blocks(bytes, recipe).map(|mut tables| tables.remove(0))
}

/// Like [`parse`], but returns **one table per data block**
/// (`recipe.block_split`): a file that chains several spectra gives several
/// tables, each with its own header. Without splitting, a single table. A
/// block with no data is dropped; the import fails only if no block has any.
pub fn parse_blocks(bytes: &[u8], recipe: &ImportRecipe) -> Result<Vec<ParsedTable>, ImportError> {
    if bytes.is_empty() {
        return Err(ImportError::Empty);
    }
    let text = crate::decode::decode_with(bytes, recipe.encoding)?;
    let mut tables = Vec::new();
    let mut last_error = ImportError::Empty;
    for block in crate::layout::split_blocks(&text, &recipe.block_split) {
        match crate::layout::analyze(block, recipe) {
            Ok(layout) => tables.push(table_from_layout(&layout, recipe)),
            Err(error) => last_error = error,
        }
    }
    if tables.is_empty() {
        Err(last_error)
    } else {
        Ok(tables)
    }
}

/// Builds the table of a [`crate::layout::Layout`]: columns, sub-headers,
/// metadata, warnings.
pub(crate) fn table_from_layout(
    layout: &crate::layout::Layout<'_>,
    recipe: &ImportRecipe,
) -> ParsedTable {
    use crate::layout::skip_str;
    let separator = &layout.separator;
    let numbers = &layout.numbers;
    let header_lines: Vec<String> = layout
        .header
        .iter()
        .map(|&i| layout.raw[i].to_string())
        .collect();
    // Roles of the sub-header lines ("Long Names"/"Units"/"Comments"),
    // resolved once: each role points at the **first** line of its range (a
    // multi-line range is not spread across columns; documented limit).
    let subheader_lines = &header_lines[layout.subheader_start..];
    let role_line = |range: Option<crate::recipe::SubheaderLineRange>| {
        range.and_then(|r| subheader_lines.get(r.from.checked_sub(1)?))
    };
    let long_name_line = role_line(recipe.subheader_roles.long_name);
    let unit_line = role_line(recipe.subheader_roles.unit);
    let comment_line = role_line(recipe.subheader_roles.comment);
    let x_prime_line = role_line(recipe.subheader_roles.x_prime);

    // Column header line, if any.
    let column_names: Option<Vec<String>> = layout.column_header.map(|i| {
        separator
            .tokenize(skip_str(layout.raw[i], recipe.skip_chars))
            .into_iter()
            .map(|t| t.trim().to_string())
            .collect()
    });

    // One value per data column, in the same order, for each sub-header line
    // assigned to a role.
    let role_values = |line: Option<&String>| -> Option<Vec<String>> {
        line.map(|l| {
            separator
                .tokenize(l.as_str())
                .into_iter()
                .map(|t| t.trim().to_string())
                .collect()
        })
    };
    let long_names = role_values(long_name_line);
    let units = if recipe.subheader_roles.same_line_name_unit {
        // Same line as `long_name`: each token has the form "Name (unit)".
        // Without recognized parentheses, the whole token stays the long name
        // and the unit stays empty.
        long_names.as_ref().map(|names| {
            names
                .iter()
                .map(|n| match (n.find('('), n.find(')')) {
                    (Some(open), Some(close)) if open < close => {
                        n[open + 1..close].trim().to_string()
                    }
                    _ => String::new(),
                })
                .collect()
        })
    } else {
        role_values(unit_line)
    };
    let long_names = if recipe.subheader_roles.same_line_name_unit {
        long_names.map(|names| {
            names
                .into_iter()
                .map(|n| match n.find('(') {
                    Some(open) => n[..open].trim().to_string(),
                    None => n,
                })
                .collect()
        })
    } else {
        long_names
    };
    let comments = role_values(comment_line);
    let x_primes = role_values(x_prime_line);

    // Tokenizes and parses each data line, keeping the original token next to
    // the value: this is what `texts` returns for a cell that is not a number
    // (mixed-type columns).
    let rows: Vec<Vec<(f64, Option<String>)>> = layout
        .data
        .iter()
        .map(|&i| {
            separator
                .tokenize(skip_str(layout.raw[i], recipe.skip_chars))
                .into_iter()
                .enumerate()
                .map(|(column, t)| {
                    read_cell(t, recipe.column_kind(column), numbers, recipe.date_order)
                })
                .collect()
        })
        .collect();

    let num_cols = rows.iter().map(Vec::len).max().unwrap_or(0);
    let columns: Vec<ParsedColumn> = (0..num_cols)
        .map(|c| {
            let name = column_names
                .as_ref()
                .and_then(|names| names.get(c))
                .filter(|n| !n.is_empty())
                .cloned();
            let values = rows
                .iter()
                .map(|r| r.get(c).map_or(f64::NAN, |(v, _)| *v))
                .collect();
            let texts = rows
                .iter()
                .map(|r| r.get(c).and_then(|(_, t)| t.clone()))
                .collect();
            let pick = |values: &Option<Vec<String>>| {
                values
                    .as_ref()
                    .and_then(|v| v.get(c))
                    .filter(|n| !n.is_empty())
                    .cloned()
            };
            ParsedColumn {
                name,
                values,
                texts,
                long_name: pick(&long_names),
                unit: pick(&units),
                comment: pick(&comments),
                x_prime: x_primes
                    .as_ref()
                    .and_then(|v| v.get(c))
                    .map(|token| numbers.parse(token))
                    .filter(|value| value.is_finite()),
                datetime: recipe.column_kind(c) == crate::recipe::ColumnKind::DateTime,
            }
        })
        .collect();

    // Footer: everything after the last data line, as is.
    let last_data = layout.data.last().copied().unwrap_or(layout.start);
    let footer_lines: Vec<String> = layout.raw[(last_data + 1).min(layout.end)..layout.end]
        .iter()
        .map(|line| (*line).to_string())
        .collect();

    let mut warnings = Vec::new();
    if let Some(declared) = lookup_header_value(&header_lines, separator, "NPOINTS") {
        if let Ok(declared_n) = declared.trim().parse::<i64>() {
            let actual = rows.len() as i64;
            if declared_n != actual {
                warnings.push(format!(
                    "NPOINTS declares {declared_n} lines, {actual} read"
                ));
            }
        }
    }

    let metadata = crate::metadata::split_lines(
        header_lines
            .iter()
            .filter(|_| recipe.keep_header_metadata)
            .chain(footer_lines.iter().filter(|_| recipe.keep_footer_metadata))
            .map(String::as_str),
        separator,
        recipe,
    );

    ParsedTable {
        columns,
        warnings,
        header_lines,
        footer_lines,
        metadata,
        separator: separator.clone(),
    }
}

/// Reads a token according to the chosen kind of its column: `(value, kept
/// text)`. A single rule, for the import as for the preview (whose "text"
/// annotation is derived from the second element).
pub(crate) fn read_cell(
    token: &str,
    kind: crate::recipe::ColumnKind,
    numbers: &crate::numbers::NumberFormat,
    date_order: crate::recipe::DateOrder,
) -> (f64, Option<String>) {
    use crate::recipe::ColumnKind;
    let trimmed = token.trim();
    match kind {
        ColumnKind::Auto => {
            let value = numbers.parse(token);
            let text = (value.is_nan() && !trimmed.is_empty() && !numbers.is_missing(token))
                .then(|| trimmed.to_string());
            (value, text)
        }
        ColumnKind::Text => (f64::NAN, (!trimmed.is_empty()).then(|| trimmed.to_string())),
        ColumnKind::DateTime => match crate::datetime::parse_datetime_with(trimmed, date_order) {
            Some(seconds) => (seconds, None),
            None => (
                f64::NAN,
                (!trimmed.is_empty() && !numbers.is_missing(token)).then(|| trimmed.to_string()),
            ),
        },
    }
}

/// Resolves `Separator::Auto`/`Whitespace` into a concrete separator, based
/// on the first line remaining after the header and the removal of
/// `skip_chars`, **column names line included**. `pub(crate)`: called by
/// [`crate::layout::analyze`], the only place where this decision is made for
/// the import and for the preview (issue #23).
///
/// With the comma as decimal, the comma is never a candidate: it cannot be
/// both separator and decimal, and two decimal commas on a line would
/// otherwise beat a single tab.
///
/// `Err(WhitespaceWithSpaceThousands)` if the resolved separator is
/// "whitespace" and the thousands separator is the space: the combination is
/// ambiguous and rejected.
pub(crate) fn resolve_separator(
    separator: &Separator,
    first_line: &str,
    decimal: DecimalStyle,
    thousands: Option<char>,
) -> Result<EffectiveSeparator, ImportError> {
    let whitespace = EffectiveSeparator::Whitespace;
    let resolved = match separator {
        Separator::Comma => EffectiveSeparator::Char(','),
        Separator::Semicolon => EffectiveSeparator::Char(';'),
        Separator::Tab => EffectiveSeparator::Char('\t'),
        Separator::Whitespace => whitespace,
        Separator::Custom(c) => EffectiveSeparator::Char(*c),
        Separator::FixedWidth(widths) => EffectiveSeparator::Fixed(widths.clone()),
        Separator::Auto => {
            let candidates = [
                (';', first_line.matches(';').count()),
                ('\t', first_line.matches('\t').count()),
                (
                    ',',
                    if decimal == DecimalStyle::Comma {
                        0
                    } else {
                        first_line.matches(',').count()
                    },
                ),
            ];
            match candidates.into_iter().max_by_key(|(_, count)| *count) {
                Some((c, count)) if count > 0 => EffectiveSeparator::Char(c),
                _ => whitespace,
            }
        }
    };
    if resolved == EffectiveSeparator::Whitespace && thousands == Some(' ') {
        return Err(ImportError::WhitespaceWithSpaceThousands);
    }
    Ok(resolved)
}

/// Resolves the series name according to `recipe.name_source`: `Filename`
/// returns `file_stem` as is; `HeaderKey(key)` returns
/// `table.header_value(key)`, or falls back to `file_stem` if the key is
/// absent from the header. Never a hard failure for a missing name
/// (invariant 10).
pub fn resolve_series_name(recipe: &ImportRecipe, table: &ParsedTable, file_stem: &str) -> String {
    match &recipe.name_source {
        NameSource::Filename => file_stem.to_string(),
        NameSource::HeaderKey(key) => table
            .header_value(key)
            .unwrap_or_else(|| file_stem.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::recipe::{HeaderSkip, ImportRecipe};

    #[test]
    fn binary_file_gives_not_text_without_panicking() {
        let bytes: &[u8] = &[0xff, 0xfe, 0x00, 0x01, 0x02];
        let recipe = ImportRecipe::new("default");
        assert_eq!(parse(bytes, &recipe), Err(ImportError::NotText));
    }

    #[test]
    fn empty_file_gives_empty() {
        let recipe = ImportRecipe::new("default");
        assert_eq!(parse(b"", &recipe), Err(ImportError::Empty));
    }

    #[test]
    fn a_shorter_line_is_padded_with_nan() {
        let recipe = ImportRecipe::new("default");
        let table = parse(b"1,2,3\n4,5\n6,7,8\n", &recipe).unwrap();
        assert_eq!(table.columns.len(), 3);
        assert_eq!(table.row_count(), 3);
        assert_eq!(table.columns[2].values[0], 3.0);
        assert!(table.columns[2].values[1].is_nan());
        assert_eq!(table.columns[2].values[2], 8.0);
    }

    #[test]
    fn missing_marker_gives_no_data_rows() {
        let mut recipe = ImportRecipe::new("default");
        recipe.header = HeaderSkip::UntilMarker("XYDATA".into());
        let table = parse(b"one\ntwo\nthree\n", &recipe);
        assert_eq!(table, Err(ImportError::NoDataRows));
    }

    #[test]
    fn resolve_series_name_falls_back_to_filename_when_the_key_is_absent() {
        let mut recipe = ImportRecipe::new("default");
        recipe.name_source = NameSource::HeaderKey("TITLE".into());
        let table = parse(b"1,2\n3,4\n", &recipe).unwrap();
        assert_eq!(resolve_series_name(&recipe, &table, "my_file"), "my_file");
    }
}

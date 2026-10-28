//! Live preview of how a file is split by an [`crate::ImportRecipe`], in the
//! manner of OriginLab's ASCII import wizard: classifies **each line** by
//! role (main header, sub-header with its role, column header, data, footer)
//! exactly as [`crate::parse::parse`] would treat it, so that the panel that
//! shows this preview updates live at every settings change. See
//! `docs/chantier-import-previsualisation.md` (the goal of the work package
//! that added this module).
//!
//! The point of vigilance that guided the whole writing of this module: stay
//! usable on a file of several tens of thousands of lines without reparsing
//! (even less analyzing cell by cell) the whole file at every keystroke. Two
//! distinct bounds provide for it:
//!
//! - the **line splitting** (`layout::split_lines`: `\r\n`, `\n` or `\r`) is
//!   done once, on the whole file. It is `O(file size)`, incompressible to
//!   know where the lines are, but only indexes slices of the original string
//!   (no copy);
//! - **building an owned [`PreviewLine`]** (with its own `String`) and
//!   **tokenizing/analyzing a line cell by cell** are done only for the lines
//!   actually shown: the header and the footer (small counts, set by the
//!   user, never proportional to the file size) and at most
//!   `recipe.preview_lines` data lines, never for the rest, however long.
//!   [`FilePreview::total_data_lines`] returns the real count without having
//!   materialized every line.

use crate::numbers::NumberFormat;
use crate::parse::{resolve_separator, ImportError};
use crate::recipe::{ImportRecipe, SubheaderRoles};

/// Role given to a file line, for the preview. Vocabulary modeled on the
/// legend of OriginLab's ASCII import wizard (`S`/`L`/`U`/`C`/`MH`/`SH`, see
/// [`LineRole::code`]). `P` ("System/User Parameters") has no StokesForge
/// equivalent, see the top-level doc of [`crate::recipe::SubheaderRoles`].
/// `Footer` is an extra StokesForge role: OriginLab does not distinguish it in
/// its legend.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineRole {
    /// Blank line at one end of the file, removed before any analysis (same
    /// rule as [`crate::parse::parse`]); never counted in the header or the
    /// data. Internal: never in a [`PreviewLine`] returned by [`preview`],
    /// only the default value of the working array before classification (see
    /// its code).
    Blank,
    /// "Main" header (`HeaderLayout::main_header_lines`, or the whole classic
    /// header `HeaderSkip::Lines`/`UntilMarker` when the recipe does not use
    /// the structured model; the two are not distinguished in that case, see
    /// the doc of [`crate::recipe::HeaderLayout`]).
    MainHeader,
    /// Sub-header line with none of the three recognized roles assigned.
    Subheader,
    /// Sub-header line assigned to the `long_name` role.
    SubheaderLongName,
    /// Sub-header line assigned to the `unit` role.
    SubheaderUnit,
    /// Sub-header line assigned to the `comment` role.
    SubheaderComment,
    /// Sub-header line assigned to the `x_prime` role (#164).
    SubheaderXPrime,
    /// Line of short column names (`has_column_header_row`): OriginLab's `S`
    /// ("Short Name").
    ColumnHeaderRow,
    /// Data line.
    Data,
    /// Footer: `footer_skip_lines`, and everything after the end of the data
    /// ([`crate::recipe::DataEnd`]).
    Footer,
    /// Comment line (`comment_prefix`): never data.
    Comment,
    /// Data line skipped at the start (`skip_data_rows`).
    Skipped,
}

impl LineRole {
    /// Code shown in the preview, OriginLab vocabulary where it exists.
    pub fn code(self) -> &'static str {
        match self {
            LineRole::Blank => "",
            LineRole::MainHeader => "MH",
            LineRole::Subheader => "SH",
            LineRole::SubheaderLongName => "L",
            LineRole::SubheaderUnit => "U",
            LineRole::SubheaderComment => "C",
            LineRole::SubheaderXPrime => "X'",
            LineRole::ColumnHeaderRow => "S",
            LineRole::Data => "",
            LineRole::Footer => "F",
            LineRole::Comment => "#",
            LineRole::Skipped => "x",
        }
    }
}

/// A cell of a data line shown in the preview.
#[derive(Debug, Clone, PartialEq)]
pub struct PreviewCell {
    /// Original token, trimmed.
    pub text: String,
    /// True if this cell would become kept text rather than a silent `NaN` in
    /// a mixed column (non-empty token that does not parse as a number); same
    /// rule as [`crate::parse::ParsedColumn::texts`].
    pub is_text: bool,
}

/// A file line as the preview shows it.
#[derive(Debug, Clone, PartialEq)]
pub struct PreviewLine {
    /// Line number in the original file, 1-based.
    pub number: usize,
    pub role: LineRole,
    pub text: String,
    /// Cell by cell, only for a data line within the `recipe.preview_lines`
    /// window; `None` for any other line, including a data line beyond this
    /// window (see the module's top-level doc).
    pub cells: Option<Vec<PreviewCell>>,
}

/// The result of a preview: the lines to show, plus the real count of data
/// lines (whether shown or not).
#[derive(Debug, Clone, PartialEq)]
pub struct FilePreview {
    pub lines: Vec<PreviewLine>,
    pub total_data_lines: usize,
    /// Number of data blocks in the file (`recipe.block_split`); the preview
    /// shows the first.
    pub block_count: usize,
}

/// Maximum number of non-data lines (header, footer, comments, skipped lines)
/// shown by the preview: a footer of several million lines must not turn into
/// as many allocations.
const MAX_OTHER_LINES: usize = 500;

/// Classifies each line of `bytes` according to `recipe`, up to
/// `recipe.preview_lines` data lines. Same failure conditions as
/// [`crate::parse::parse`]: `Err` in exactly the same cases, with the same
/// meaning. Both rely on the **same** split ([`crate::layout::analyze`]), so
/// the preview shows what the import will do.
pub fn preview(bytes: &[u8], recipe: &ImportRecipe) -> Result<FilePreview, ImportError> {
    if bytes.is_empty() {
        return Err(ImportError::Empty);
    }
    let text = crate::decode::decode_with(bytes, recipe.encoding)?;
    let blocks = crate::layout::split_blocks(&text, &recipe.block_split);
    let block_count = blocks.len().max(1);
    let first_block = blocks.first().copied().unwrap_or(&text);
    let layout = crate::layout::analyze(first_block, recipe)?;
    let total_data_lines = layout.data.len();
    let shown_data_lines = total_data_lines.min(recipe.preview_lines);

    // Emits an owned `PreviewLine` only for the lines actually shown: header,
    // footer and other zones (counts bounded by `MAX_OTHER_LINES`) and at
    // most `preview_lines` data lines, never for the rest; see the module's
    // top-level doc.
    let mut lines = Vec::new();
    let (mut data_seen, mut others) = (0usize, 0usize);
    for i in layout.start..layout.end {
        let role = layout.roles[i];
        let with_cells = role == LineRole::Data;
        if with_cells {
            if data_seen >= shown_data_lines {
                continue;
            }
            data_seen += 1;
        } else {
            if others >= MAX_OTHER_LINES {
                continue;
            }
            others += 1;
        }
        let cells = with_cells.then(|| {
            layout
                .separator
                .tokenize(crate::layout::skip_str(layout.raw[i], recipe.skip_chars))
                .into_iter()
                .enumerate()
                .map(|(column, t)| {
                    let (_, text) = crate::parse::read_cell(
                        t,
                        recipe.column_kind(column),
                        &layout.numbers,
                        recipe.date_order,
                    );
                    PreviewCell {
                        text: t.trim().to_string(),
                        is_text: text.is_some(),
                    }
                })
                .collect()
        });
        lines.push(PreviewLine {
            number: i + 1,
            role,
            text: layout.raw[i].to_string(),
            cells,
        });
    }

    Ok(FilePreview {
        lines,
        total_data_lines,
        block_count,
    })
}

/// Role of a sub-header line, 1-based **within the sub-header block**
/// (independent of the number of main header lines before it). A line that
/// falls in several assigned ranges (unlikely but possible: nothing prevents
/// the user from overlapping two roles) follows the priority `long_name` >
/// `unit` > `comment` > `x_prime`, documented here rather than carrying
/// several roles at once, which [`LineRole`] does not model.
pub(crate) fn subheader_line_role(sub_index: usize, roles: &SubheaderRoles) -> LineRole {
    let contains = |range: Option<crate::recipe::SubheaderLineRange>| {
        range.is_some_and(|r| r.from <= sub_index && sub_index <= r.to)
    };
    if contains(roles.long_name) {
        LineRole::SubheaderLongName
    } else if !roles.same_line_name_unit && contains(roles.unit) {
        LineRole::SubheaderUnit
    } else if contains(roles.comment) {
        LineRole::SubheaderComment
    } else if contains(roles.x_prime) {
        LineRole::SubheaderXPrime
    } else {
        LineRole::Subheader
    }
}

/// Automatically detects `(main_header_lines, subheader_lines)` from the
/// file content: OriginLab's re-evaluation button (`auto_detect`, see
/// [`crate::recipe::HeaderLayout`]).
///
/// Two-pass heuristic, on at most the first 500 non-blank lines of the file
/// (a documented cap, not an error: a header longer than that does not exist
/// in practice in this domain, and it bounds the cost regardless of the file
/// size; same spirit as the six caps of
/// `docs/archive/rustrewrite/questions-ouvertes.md` §11):
///
/// 1. **Main header**: from the start, as long as a line does not have the
///    same number of fields (once tokenized by `separator`) as the reference
///    line (the last line examined, usually a data line);
/// 2. **Sub-header**: from there, as long as a line has the right number of
///    fields but at least one of its tokens does not parse as a number (a
///    sub-header line "aligns" its fields with the columns without being
///    numeric data itself: typically a column name or a unit).
///
/// Known limit, documented rather than fixed: a file with no data line among
/// the 500 examined (abnormally long header, or a file with only text) makes
/// the detection degenerate to "everything is main header, nothing is
/// sub-header". Not a panic, just a not very useful detection, to correct by
/// hand like any automatic detection that is wrong.
pub fn detect_header_layout(
    bytes: &[u8],
    separator: crate::recipe::Separator,
    decimal: crate::recipe::DecimalStyle,
) -> crate::recipe::HeaderLayout {
    const SCAN_CAP: usize = 500;
    let Ok(text) = crate::decode::decode_text(bytes) else {
        return crate::recipe::HeaderLayout::default();
    };
    let lines: Vec<&str> = crate::layout::split_lines(&text)
        .into_iter()
        .filter(|l| !l.trim().is_empty())
        .take(SCAN_CAP)
        .collect();
    if lines.is_empty() {
        return crate::recipe::HeaderLayout::default();
    }
    let numbers = NumberFormat::new(decimal, None, &[], lines.iter().copied());
    // Without thousands (`None`): `resolve_separator` never rejects here.
    let Ok(effective) =
        resolve_separator(&separator, lines[lines.len() - 1], numbers.decimal, None)
    else {
        return crate::recipe::HeaderLayout::default();
    };
    let ref_count = effective.tokenize(lines[lines.len() - 1]).len();

    let mut i = 0usize;
    let mut main = 0usize;
    while i < lines.len() && effective.tokenize(lines[i]).len() != ref_count {
        main += 1;
        i += 1;
    }
    let mut sub = 0usize;
    while i < lines.len() {
        let tokens = effective.tokenize(lines[i]);
        let all_numeric = tokens
            .iter()
            .all(|t| t.trim().is_empty() || numbers.is_numeric(t));
        if all_numeric {
            break;
        }
        sub += 1;
        i += 1;
    }
    crate::recipe::HeaderLayout {
        main_header_lines: main,
        subheader_lines: sub,
        auto_detect: true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::recipe::{
        HeaderLayout, ImportRecipe, Separator, SubheaderLineRange, SubheaderRoles,
    };

    #[test]
    fn without_structure_settings_everything_is_data() {
        let recipe = ImportRecipe::new("default");
        let result = preview(b"1,2,3\n4,5,6\n", &recipe).unwrap();
        assert_eq!(result.total_data_lines, 2);
        assert!(result.lines.iter().all(|l| l.role == LineRole::Data));
        assert_eq!(result.lines[0].cells.as_ref().unwrap().len(), 3);
    }

    #[test]
    fn structured_header_classifies_main_and_sub_header() {
        let mut recipe = ImportRecipe::new("default");
        recipe.header_layout = HeaderLayout {
            main_header_lines: 1,
            subheader_lines: 2,
            auto_detect: false,
        };
        recipe.subheader_roles = SubheaderRoles {
            long_name: Some(SubheaderLineRange::single(1)),
            unit: Some(SubheaderLineRange::single(2)),
            comment: None,
            x_prime: None,
            same_line_name_unit: false,
        };
        let bytes = b"Instrument XYZ\nTemperature,Pressure\ndegC,bar\n1,2\n3,4\n";
        let result = preview(bytes, &recipe).unwrap();
        let roles: Vec<LineRole> = result.lines.iter().map(|l| l.role).collect();
        assert_eq!(
            roles,
            vec![
                LineRole::MainHeader,
                LineRole::SubheaderLongName,
                LineRole::SubheaderUnit,
                LineRole::Data,
                LineRole::Data,
            ]
        );
        assert_eq!(result.total_data_lines, 2);
    }

    #[test]
    fn non_numeric_cell_is_annotated_as_text() {
        let recipe = ImportRecipe::new("default");
        let result = preview(b"1,n/a,3\n", &recipe).unwrap();
        let cells = result.lines[0].cells.as_ref().unwrap();
        assert!(!cells[0].is_text);
        assert!(cells[1].is_text);
        assert_eq!(cells[1].text, "n/a");
        assert!(!cells[2].is_text);
    }

    #[test]
    fn preview_lines_bounds_the_analyzed_cells() {
        let mut recipe = ImportRecipe::new("default");
        recipe.preview_lines = 2;
        let bytes = b"1\n2\n3\n4\n5\n";
        let result = preview(bytes, &recipe).unwrap();
        assert_eq!(result.total_data_lines, 5);
        let with_cells = result.lines.iter().filter(|l| l.cells.is_some()).count();
        assert_eq!(with_cells, 2);
        // Lines beyond the window are not even emitted: cost bounded
        // regardless of the real number of data lines.
        assert_eq!(result.lines.len(), 2);
    }

    #[test]
    fn missing_marker_gives_the_same_error_as_parse() {
        let mut recipe = ImportRecipe::new("default");
        recipe.header = crate::recipe::HeaderSkip::UntilMarker("XYDATA".into());
        let result = preview(b"one\ntwo\nthree\n", &recipe);
        assert_eq!(result, Err(ImportError::NoDataRows));
    }

    #[test]
    fn detects_header_and_sub_header_on_a_plausible_file() {
        let bytes =
            b"Instrument XYZ\nDate: 2024-01-01\nTemperature,Pressure\ndegC,bar\n1,2\n3,4\n5,6\n";
        let layout =
            detect_header_layout(bytes, Separator::Comma, crate::recipe::DecimalStyle::Dot);
        assert_eq!(layout.main_header_lines, 2);
        assert_eq!(layout.subheader_lines, 2);
        assert!(layout.auto_detect);
    }

    #[test]
    fn detection_on_a_binary_file_does_not_panic() {
        let layout = detect_header_layout(
            &[0xff, 0xfe, 0x00],
            Separator::Auto,
            crate::recipe::DecimalStyle::Dot,
        );
        assert_eq!(layout, crate::recipe::HeaderLayout::default());
    }
}

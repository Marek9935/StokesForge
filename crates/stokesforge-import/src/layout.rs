//! The split of a file into zones (header, sub-header, names line, data,
//! footer, comments), computed **once** and shared by
//! [`crate::parse::parse`] (which draws the columns from it) and
//! [`crate::preview::preview`] (which shows its roles). The two can no longer
//! diverge (issue #23: each rewrote the same sequence of steps, with
//! different inputs).

use crate::numbers::NumberFormat;
use crate::parse::{resolve_separator, EffectiveSeparator, ImportError};
use crate::preview::LineRole;
use crate::recipe::{HeaderSkip, ImportRecipe};

/// The split of a text according to a recipe.
pub(crate) struct Layout<'a> {
    /// All the lines of the text, blank ones included (terminators removed).
    pub raw: Vec<&'a str>,
    /// First non-blank line, and end (exclusive) after removing trailing
    /// blank lines: the analyzed zone.
    pub start: usize,
    pub end: usize,
    /// Role of each line of `raw`; `Blank` outside `start..end`.
    pub roles: Vec<LineRole>,
    /// Indices (in `raw`) of the header lines, main then sub-header.
    pub header: Vec<usize>,
    /// Position, in `header`, of the first sub-header line.
    pub subheader_start: usize,
    /// Index of the column names line, if the recipe asks for one.
    pub column_header: Option<usize>,
    /// Indices of the kept data lines, in order.
    pub data: Vec<usize>,
    pub separator: EffectiveSeparator,
    pub numbers: NumberFormat,
}

/// **The** line splitting: `\r\n`, `\n` and a lone `\r` (old Macs) end a
/// line. A text that ends with a terminator gives a last empty line (blank,
/// removed afterwards). Used by [`analyze`] and by
/// `preview::detect_header_layout`: never another splitter.
pub(crate) fn split_lines(text: &str) -> Vec<&str> {
    let bytes = text.as_bytes();
    let mut lines = Vec::new();
    let mut start = 0usize;
    let mut i = 0usize;
    while i < bytes.len() {
        match bytes[i] {
            b'\n' => {
                lines.push(&text[start..i]);
                start = i + 1;
            }
            b'\r' => {
                lines.push(&text[start..i]);
                if bytes.get(i + 1) == Some(&b'\n') {
                    i += 1;
                }
                start = i + 1;
            }
            _ => {}
        }
        i += 1;
    }
    lines.push(&text[start..]);
    lines
}

/// `line` without its first `skip_chars` characters.
pub(crate) fn skip_str(line: &str, skip_chars: usize) -> &str {
    if skip_chars == 0 {
        return line;
    }
    match line.char_indices().nth(skip_chars) {
        Some((byte_idx, _)) => &line[byte_idx..],
        None => "",
    }
}

fn is_comment(line: &str, prefix: &str) -> bool {
    !prefix.is_empty() && line.trim_start().starts_with(prefix)
}

/// Splits `text` according to `recipe`. Same failures as
/// [`crate::parse::parse`]: empty file, or no data line left after the header,
/// the footer, the names line or the skipped lines.
pub(crate) fn analyze<'a>(text: &'a str, recipe: &ImportRecipe) -> Result<Layout<'a>, ImportError> {
    let raw = split_lines(text);
    // Blank lines at both ends only: an empty line in the middle stays in
    // place (it ends up all NaN rather than quietly shifting the following
    // ones).
    let mut start = 0usize;
    while start < raw.len() && raw[start].trim().is_empty() {
        start += 1;
    }
    let mut end = raw.len();
    while end > start && raw[end - 1].trim().is_empty() {
        end -= 1;
    }
    if start >= end {
        return Err(ImportError::Empty);
    }

    let mut roles = vec![LineRole::Blank; raw.len()];
    let mut header: Vec<usize> = Vec::new();
    let mut idx = start;
    let subheader_start;
    // Two header modes: as soon as `header_layout` counts a line, it
    // replaces `recipe.header` (see `docs/import-preview-work-package.md`).
    if recipe.header_layout.main_header_lines > 0 || recipe.header_layout.subheader_lines > 0 {
        let main_n = recipe.header_layout.main_header_lines.min(end - idx);
        for _ in 0..main_n {
            roles[idx] = LineRole::MainHeader;
            header.push(idx);
            idx += 1;
        }
        subheader_start = header.len();
        let sub_n = recipe.header_layout.subheader_lines.min(end - idx);
        for s in 0..sub_n {
            roles[idx] = crate::preview::subheader_line_role(s + 1, &recipe.subheader_roles);
            header.push(idx);
            idx += 1;
        }
    } else {
        match &recipe.header {
            HeaderSkip::None => {}
            HeaderSkip::Lines(n) => {
                for _ in 0..(*n).min(end - idx) {
                    roles[idx] = LineRole::MainHeader;
                    header.push(idx);
                    idx += 1;
                }
            }
            HeaderSkip::UntilMarker(marker) => {
                let marker = marker.trim().to_lowercase();
                let mut found = false;
                while idx < end {
                    roles[idx] = LineRole::MainHeader;
                    header.push(idx);
                    let matched = raw[idx].trim().to_lowercase() == marker;
                    idx += 1;
                    if matched {
                        found = true;
                        break;
                    }
                }
                if !found {
                    return Err(ImportError::NoDataRows);
                }
            }
        }
        subheader_start = header.len();
    }

    let footer_start = end - recipe.footer_skip_lines.min(end - idx);
    for role in roles.iter_mut().take(end).skip(footer_start) {
        *role = LineRole::Footer;
    }
    if idx >= footer_start {
        return Err(ImportError::NoDataRows);
    }

    // Remaining lines, comments excluded.
    let mut remaining: Vec<usize> = Vec::with_capacity(footer_start - idx);
    for i in idx..footer_start {
        if is_comment(skip_str(raw[i], recipe.skip_chars), &recipe.comment_prefix)
            || is_comment(raw[i], &recipe.comment_prefix)
        {
            roles[i] = LineRole::Comment;
        } else {
            remaining.push(i);
        }
    }
    if remaining.is_empty() {
        return Err(ImportError::NoDataRows);
    }

    // Number format and separator: resolved **on the same lines** for the
    // import and the preview: the first remaining lines (names line
    // included), prefix removed.
    let numbers = NumberFormat::resolve(
        recipe,
        remaining
            .iter()
            .map(|&i| skip_str(raw[i], recipe.skip_chars)),
    );
    let separator = resolve_separator(
        &recipe.separator,
        skip_str(raw[remaining[0]], recipe.skip_chars),
        numbers.decimal,
        numbers.thousands(),
    )?;

    let column_header = if recipe.has_column_header_row {
        let i = remaining.remove(0);
        roles[i] = LineRole::ColumnHeaderRow;
        Some(i)
    } else {
        None
    };
    // Line range: the first `skip_data_rows` are ignored.
    let skipped = recipe.skip_data_rows.min(remaining.len());
    for &i in &remaining[..skipped] {
        roles[i] = LineRole::Skipped;
    }
    let candidates = &remaining[skipped..];
    if candidates.is_empty() {
        return Err(ImportError::NoDataRows);
    }

    // Data end: the cut, then the footer.
    let kept = crate::data_end::data_len(
        candidates
            .iter()
            .map(|&i| skip_str(raw[i], recipe.skip_chars)),
        candidates.len(),
        &recipe.data_end,
        &separator,
        &numbers,
        recipe.column_kind(0),
        recipe.date_order,
    );
    if kept == 0 {
        return Err(ImportError::NoDataRows);
    }
    let data: Vec<usize> = candidates[..kept].to_vec();
    for &i in &data {
        roles[i] = LineRole::Data;
    }
    let last_data = data[kept - 1];
    for role in roles.iter_mut().take(end).skip(last_data + 1) {
        if matches!(*role, LineRole::Blank | LineRole::Data) {
            *role = LineRole::Footer;
        }
    }
    // Comments beyond the last data line stay comments; any other remaining
    // line is footer.
    Ok(Layout {
        raw,
        start,
        end,
        roles,
        header,
        subheader_start,
        column_header,
        data,
        separator,
        numbers,
    })
}

/// Splits `text` into blocks according to `split`: **contiguous** slices of
/// the text, in order. `BlockSplit::None` returns the whole text. A block
/// with only blank lines does not exist.
pub(crate) fn split_blocks<'a>(text: &'a str, split: &crate::recipe::BlockSplit) -> Vec<&'a str> {
    use crate::recipe::BlockSplit;
    if matches!(split, BlockSplit::None) {
        return vec![text];
    }
    // Start (byte) of each line, and the line text without its `\r`/`\n`.
    let mut lines: Vec<(usize, &str)> = Vec::new();
    let mut offset = 0usize;
    for line in text.split_inclusive('\n') {
        lines.push((offset, line.trim_end_matches(['\r', '\n'])));
        offset += line.len();
    }
    let mut starts: Vec<usize> = vec![0];
    match split {
        BlockSplit::None => {}
        BlockSplit::BeforeLine(marker) => {
            let marker = marker.trim().to_lowercase();
            // The first marker does not open a block: what precedes it (a
            // preface) stays with the first block. Each following one opens one.
            let mut seen_marker = false;
            for (index, (_, line)) in lines.iter().enumerate() {
                let is_marker =
                    !marker.is_empty() && line.trim().to_lowercase().starts_with(&marker);
                if is_marker {
                    if seen_marker {
                        starts.push(index);
                    }
                    seen_marker = true;
                }
            }
        }
        BlockSplit::BlankLines => {
            let mut in_blank_run = false;
            let mut seen_content = false;
            for (index, (_, line)) in lines.iter().enumerate() {
                if line.trim().is_empty() {
                    if seen_content {
                        in_blank_run = true;
                    }
                } else {
                    if in_blank_run {
                        starts.push(index);
                        in_blank_run = false;
                    }
                    seen_content = true;
                }
            }
        }
    }
    let mut blocks = Vec::new();
    for (position, &first_line) in starts.iter().enumerate() {
        let begin = lines
            .get(first_line)
            .map_or(text.len(), |(offset, _)| *offset);
        let end = match starts.get(position + 1) {
            Some(&next) => lines.get(next).map_or(text.len(), |(offset, _)| *offset),
            None => text.len(),
        };
        let block = &text[begin..end];
        if !block.trim().is_empty() {
            blocks.push(block);
        }
    }
    blocks
}

#[cfg(test)]
mod line_tests {
    use super::split_lines;

    #[test]
    fn cr_lf_and_lone_cr_end_a_line() {
        assert_eq!(split_lines("a\rb\rc"), vec!["a", "b", "c"]);
        assert_eq!(split_lines("a\r\nb"), vec!["a", "b"]);
        assert_eq!(split_lines("a\nb\r\n\rc\n"), vec!["a", "b", "", "c", ""]);
        assert_eq!(split_lines(""), vec![""]);
    }
}

#[cfg(test)]
mod block_tests {
    use super::split_blocks;
    use crate::recipe::BlockSplit;

    #[test]
    fn blocks_before_a_marker_line() {
        let text = "TITLE;a\nX\n1;2\nTITLE;b\nX\n3;4\n";
        let blocks = split_blocks(text, &BlockSplit::BeforeLine("title".into()));
        assert_eq!(blocks, vec!["TITLE;a\nX\n1;2\n", "TITLE;b\nX\n3;4\n"]);
        // Text before the first marker stays in the first block.
        let text = "préface\nTITLE;a\n1;2\nTITLE;b\n3;4\n";
        let blocks = split_blocks(text, &BlockSplit::BeforeLine("TITLE".into()));
        assert_eq!(
            blocks,
            vec!["préface\nTITLE;a\n1;2\n", "TITLE;b\n3;4\n"],
            "the preface stays with the first block"
        );
    }

    #[test]
    fn blocks_separated_by_blank_lines() {
        let text = "1;2\n3;4\n\n\n5;6\n\n7;8\n";
        let blocks = split_blocks(text, &BlockSplit::BlankLines);
        assert_eq!(blocks, vec!["1;2\n3;4\n\n\n", "5;6\n\n", "7;8\n"]);
        assert_eq!(split_blocks(text, &BlockSplit::None), vec![text]);
    }
}

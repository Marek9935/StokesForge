//! Tab-separated text (TSV) of cells — the format the clipboard exchanges
//! with a spreadsheet (Excel, LibreOffice) (#259).
//!
//! A cell stays as it is as long as it contains no tab, no
//! line break, no quote; otherwise it is wrapped in quotes and its
//! quotes are doubled (`a"b` → `"a""b"`). A row ends with `\n`
//! (the last one too). This is the format spreadsheets read and write:
//! a multi-line text therefore no longer shifts the following cells.
//!
//! This module knows neither the selection, nor how a missing cell is shown,
//! nor egui: it turns rows of ready-made strings into text. Choosing which
//! cells are copied and turning them into text stay with the caller.
//!
//! # Reading ([`decode`])
//!
//! - line ending `\n` or `\r\n` (a spreadsheet on Windows writes `\r\n`);
//! - a **trailing** line ending does not create an extra row;
//! - inside a quoted cell, everything is kept as is (`\r\n`
//!   included) and `""` becomes `"` again;
//! - a quote that does not open the cell (`5" tube`) is an ordinary
//!   character; a quote never closed runs to the end of the text —
//!   a malformed third-party clipboard loses nothing.
//!
//! `decode("")` is empty; `decode(&encode(rows)) == rows` for any row
//! of at least one cell (a row with no cell is written `\n`, which reads back as
//! a row of one empty cell).

/// True if `cell` must be wrapped in quotes to survive
/// [`decode`]: tab, line break (`\n` or `\r`) or quote.
fn needs_quotes(cell: &str) -> bool {
    cell.contains(['\t', '\n', '\r', '"'])
}

/// One cell, quoted if [`needs_quotes`] — with no delimiter or line
/// ending. This is also what is pushed to the clipboard for a single cell.
pub fn encode_cell(cell: &str) -> String {
    if needs_quotes(cell) {
        format!("\"{}\"", cell.replace('"', "\"\""))
    } else {
        cell.to_string()
    }
}

/// Turns `rows` into tab-separated text: `\t` between cells, `\n` after each
/// row, the last one included.
pub fn encode(rows: &[Vec<String>]) -> String {
    let mut out = String::new();
    for row in rows {
        for (index, cell) in row.iter().enumerate() {
            if index > 0 {
                out.push('\t');
            }
            out.push_str(&encode_cell(cell));
        }
        out.push('\n');
    }
    out
}

/// Reads tab-separated text back (see the module doc). An empty text gives no
/// row; a text that does not end with a line ending keeps its last
/// row.
pub fn decode(text: &str) -> Vec<Vec<String>> {
    let mut rows: Vec<Vec<String>> = Vec::new();
    let mut row: Vec<String> = Vec::new();
    let mut cell = String::new();
    // At the start of a cell (start of text, of a row, or after `\t`): the only
    // place where a quote opens a quoted cell.
    let mut at_cell_start = true;
    // True once the current row has received a character or a delimiter:
    // tells "end of text after a line ending" (nothing to add)
    // from a last row with no line ending.
    let mut pending = false;
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' if at_cell_start => {
                pending = true;
                at_cell_start = false;
                while let Some(inner) = chars.next() {
                    if inner != '"' {
                        cell.push(inner);
                    } else if chars.peek() == Some(&'"') {
                        chars.next();
                        cell.push('"');
                    } else {
                        break;
                    }
                }
            }
            '\t' => {
                row.push(std::mem::take(&mut cell));
                at_cell_start = true;
                pending = true;
            }
            '\n' => {
                row.push(std::mem::take(&mut cell));
                rows.push(std::mem::take(&mut row));
                at_cell_start = true;
                pending = false;
            }
            // `\r\n`: the `\r` is not part of the cell.
            '\r' if chars.peek() == Some(&'\n') => {}
            other => {
                cell.push(other);
                at_cell_start = false;
                pending = true;
            }
        }
    }
    if pending {
        row.push(cell);
        rows.push(row);
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::{decode, encode, encode_cell};

    fn rows(data: &[&[&str]]) -> Vec<Vec<String>> {
        data.iter()
            .map(|row| row.iter().map(|c| c.to_string()).collect())
            .collect()
    }

    fn roundtrip(data: &[&[&str]]) {
        let rows = rows(data);
        let text = encode(&rows);
        assert_eq!(decode(&text), rows, "intermediate text: {text:?}");
    }

    #[test]
    fn a_plain_cell_stays_as_it_is() {
        assert_eq!(encode(&rows(&[&["a", "b"], &["c", "d"]])), "a\tb\nc\td\n");
        roundtrip(&[&["a", "b"], &["c", "d"]]);
    }

    #[test]
    fn a_tab_in_a_cell_shifts_nothing() {
        assert_eq!(encode(&rows(&[&["a\tb", "c"]])), "\"a\tb\"\tc\n");
        roundtrip(&[&["a\tb", "c"]]);
    }

    #[test]
    fn a_line_break_in_a_cell_shifts_nothing() {
        assert_eq!(encode(&rows(&[&["a\nb", "c"]])), "\"a\nb\"\tc\n");
        roundtrip(&[&["a\nb", "c"], &["d", "e"]]);
        roundtrip(&[&["a\r\nb", "c"], &["d", "e"]]);
        roundtrip(&[&["a\rb", "c"]]);
        roundtrip(&[&["end\n", "\nstart"]]);
    }

    #[test]
    fn a_quote_is_doubled() {
        assert_eq!(
            encode(&rows(&[&["he said \"yes\""]])),
            "\"he said \"\"yes\"\"\"\n"
        );
        roundtrip(&[&["he said \"yes\""]]);
        roundtrip(&[&["\""]]);
        roundtrip(&[&["\"", "\"\""]]);
        roundtrip(&[&["\"x"]]);
    }

    #[test]
    fn a_non_breaking_space_is_kept() {
        roundtrip(&[&["\u{00A0}", "a\u{00A0}b", "\u{00A0}\n\u{00A0}"]]);
        assert_eq!(encode(&rows(&[&["\u{00A0}"]])), "\u{00A0}\n");
    }

    #[test]
    fn empty_cells_round_trip() {
        roundtrip(&[&[""]]);
        roundtrip(&[&["", ""]]);
        roundtrip(&[&["a", ""], &["", "b"], &["", ""]]);
        roundtrip(&[&["a", "", "c"]]);
    }

    #[test]
    fn several_rows_and_columns_round_trip() {
        roundtrip(&[
            &["x", "y\tz", "w"],
            &["1", "2\n3", "\"4\""],
            &["", "\u{00A0}", "6"],
        ]);
        assert!(decode("").is_empty());
        assert_eq!(encode(&[]), "");
    }

    #[test]
    fn encode_cell_adds_neither_delimiter_nor_line_ending() {
        assert_eq!(encode_cell("a"), "a");
        assert_eq!(encode_cell("a\nb"), "\"a\nb\"");
        assert_eq!(encode_cell("a\"b"), "\"a\"\"b\"");
    }

    // -- Reading from an external spreadsheet --

    #[test]
    fn a_windows_spreadsheet_ends_its_rows_with_crlf() {
        assert_eq!(
            decode("a\tb\r\nc\td\r\n"),
            rows(&[&["a", "b"], &["c", "d"]])
        );
        // Quoted cell: the inner `\r\n` is kept.
        assert_eq!(decode("\"a\r\nb\"\tc\r\n"), rows(&[&["a\r\nb", "c"]]));
    }

    #[test]
    fn the_trailing_line_ending_does_not_create_a_row() {
        assert_eq!(decode("a\n"), rows(&[&["a"]]));
        assert_eq!(decode("a"), rows(&[&["a"]]));
        assert_eq!(decode("a\n\n"), rows(&[&["a"], &[""]]));
        assert_eq!(decode("\n"), rows(&[&[""]]));
        assert_eq!(decode("a\t"), rows(&[&["a", ""]]));
        assert_eq!(
            decode("0\t10\n0.5\t\n"),
            rows(&[&["0", "10"], &["0.5", ""]])
        );
    }

    #[test]
    fn a_quote_not_at_the_start_of_a_cell_is_ordinary() {
        assert_eq!(decode("5\" tube\tb"), rows(&[&["5\" tube", "b"]]));
        assert_eq!(decode("a\"b\"c"), rows(&[&["a\"b\"c"]]));
    }

    #[test]
    fn a_quote_never_closed_runs_to_the_end() {
        assert_eq!(decode("\"a\tb\nc"), rows(&[&["a\tb\nc"]]));
    }
}

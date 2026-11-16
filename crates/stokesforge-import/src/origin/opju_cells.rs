// SPDX-License-Identifier: GPL-3.0-or-later
//
// The FPC parameters, the segment grammar and the value tags were first
// documented by the Apache-2.0 project quantized
// (https://github.com/pquarterman17/quantized, docs/origin_project_format.md
// section 3.3, (c) 2026 Paige Quarterman). Facts only: no code or text of it
// is copied. See docs/license-and-extensions.md.

//! The cell block of an `.opju` dataset.
//!
//! ```text
//! cells   = flag  LEB nrows  item*          (until nrows cells)
//! flag    = 0x05  text cells inline: LEB len + bytes
//!         | 0x04  text cells by reference: LEB ref (0 = empty, k = byte k-1
//!                 of the string table, see below)
//! item    = FF FF  LEB m  00  numbers(m)    m numbers
//!         | FF FF  LEB m  01  texts(m)      m text cells
//!         | 01 00  text-cell                one text cell
//!         | 00 00  value-spec               one number
//! numbers = (zigzag(-k) 0C FPC(k) | zigzag(+k) value-spec)*   until m values
//! texts   = (zigzag(-k) text-cell^k | zigzag(+k) text-cell)* until m cells
//! ```
//!
//! A negative segment `-k` is `k` distinct values; a positive segment `+k` is
//! one value repeated `k` times; 0 is an error. A value spec is the tag
//! `0x64` (0.0), `0x65` (Origin's missing value) or a tag `t` with
//! `(t - 8) % 9 == 0` and `k = (t - 8) / 9` in 1..=8 followed by `k` bytes
//! (little endian) that are the top `k` bytes of the `f64` bits.
//!
//! The string table is in the column-properties blob that follows the
//! cells: the key `0x23` + `_MSER_STRINGS_PSET`, then `05 01`, `LEB n`, one
//! byte, and `n` bytes holding `u32 LE dsz` + an LZ4 block. The property-set
//! grammar around it is not known, so the key is looked up inside that
//! bounded blob only, never in the whole file. The decompressed table starts
//! with the magic `77 11 11 11`, a 24-byte header, then NUL-terminated
//! strings.
//!
//! Budget: a repeat segment turns two bytes into any number of cells, so the
//! cells of the whole file are counted ([`Context`]) and refused beyond
//! `1_000_000 + 8 * file_len`; the bytes of text cells beyond
//! `16 MiB + 8 * file_len`.

use super::dataset::{number_from_bits, ColumnBuilder};
use super::opju_codec::{lz4_block, text, Cursor, Fpc};
use super::OriginError;

const TABLE_MAGIC: [u8; 4] = [0x77, 0x11, 0x11, 0x11];
const TABLE_HEADER: usize = 24;
const TABLE_KEY: &[u8] = b"\x23_MSER_STRINGS_PSET";
const MISSING_BITS: u64 = (-1.23456789e-300f64).to_bits();

/// Budgets and the reusable FPC state, shared by every dataset of a file.
pub(super) struct Context {
    pub(super) fpc: Fpc,
    cells_limit: usize,
    cells_left: usize,
    text_limit: usize,
    text_left: usize,
}

impl Context {
    pub(super) fn new(file_len: usize) -> Self {
        let cells_limit = 1_000_000usize.saturating_add(file_len.saturating_mul(8));
        let text_limit = (16usize << 20).saturating_add(file_len.saturating_mul(8));
        Self {
            fpc: Fpc::new(),
            cells_limit,
            cells_left: cells_limit,
            text_limit,
            text_left: text_limit,
        }
    }

    fn take_cells(&mut self, n: usize) -> Result<(), OriginError> {
        self.cells_left = self
            .cells_left
            .checked_sub(n)
            .ok_or(OriginError::TooManyCells {
                limit: self.cells_limit,
            })?;
        Ok(())
    }

    fn take_text(&mut self, n: usize) -> Result<(), OriginError> {
        self.text_left = self
            .text_left
            .checked_sub(n)
            .ok_or(OriginError::TooMuchText {
                limit: self.text_limit,
            })?;
        Ok(())
    }
}

/// The cells of a dataset, text references not yet resolved.
pub(super) struct Cells {
    pub(super) column: ColumnBuilder,
    /// `(row, reference)` of the text cells stored by reference.
    refs: Vec<(usize, usize)>,
}

enum TextCell {
    Inline(String),
    Ref(usize),
}

fn value_spec(cur: &mut Cursor) -> Result<u64, OriginError> {
    let at = cur.pos();
    let tag = cur.u8()?;
    match tag {
        0x64 => Ok(0),
        0x65 => Ok(MISSING_BITS),
        t if t >= 17 && (t - 8) % 9 == 0 && (t - 8) / 9 <= 8 => {
            let k = usize::from((t - 8) / 9);
            let mut buf = [0u8; 8];
            let from = 8 - k;
            buf.get_mut(from..)
                .ok_or_else(|| Cursor::unsupported_at(at, "value width"))?
                .copy_from_slice(cur.take(k)?);
            Ok(u64::from_le_bytes(buf))
        }
        _ => Err(Cursor::unsupported_at(at, "unknown value tag")),
    }
}

fn text_cell(cur: &mut Cursor, inline: bool) -> Result<TextCell, OriginError> {
    let n = cur.leb_usize()?;
    if inline {
        Ok(TextCell::Inline(text(cur.take(n)?)))
    } else {
        Ok(TextCell::Ref(n))
    }
}

/// Appends `times` copies of a text cell.
fn push_text(
    cells: &mut Cells,
    ctx: &mut Context,
    cell: &TextCell,
    times: usize,
) -> Result<(), OriginError> {
    ctx.take_cells(times)?;
    match cell {
        TextCell::Inline(s) if s.is_empty() => {
            for _ in 0..times {
                cells.column.push(f64::NAN, None);
            }
        }
        TextCell::Inline(s) => {
            ctx.take_text(s.len().saturating_mul(times))?;
            for _ in 0..times {
                cells.column.push(f64::NAN, Some(s.clone()));
            }
        }
        TextCell::Ref(0) => {
            for _ in 0..times {
                cells.column.push(f64::NAN, None);
            }
        }
        TextCell::Ref(r) => {
            for _ in 0..times {
                cells.refs.push((cells.column.len(), *r));
                cells.column.push(f64::NAN, None);
            }
        }
    }
    Ok(())
}

/// A segment length: non-zero, within the `room` left in the run.
fn segment(cur: &mut Cursor, room: usize) -> Result<(bool, usize), OriginError> {
    let at = cur.pos();
    let z = cur.zigzag()?;
    let k = usize::try_from(z.unsigned_abs())
        .map_err(|_| Cursor::unsupported_at(at, "segment length does not fit in memory"))?;
    if k == 0 {
        return Err(Cursor::unsupported_at(at, "empty segment"));
    }
    if k > room {
        return Err(Cursor::unsupported_at(at, "segment overruns its run"));
    }
    Ok((z < 0, k))
}

fn numbers(
    cur: &mut Cursor,
    ctx: &mut Context,
    cells: &mut Cells,
    m: usize,
) -> Result<(), OriginError> {
    let mut done = 0;
    while done < m {
        let (distinct, k) = segment(cur, m - done)?;
        if distinct {
            let at = cur.pos();
            if cur.u8()? != 0x0C {
                return Err(Cursor::unsupported_at(at, "FPC segment marker expected"));
            }
            ctx.take_cells(k)?;
            let column = &mut cells.column;
            cur.fpc(&mut ctx.fpc, k, |bits| {
                column.push(number_from_bits(bits), None)
            })?;
        } else {
            let value = number_from_bits(value_spec(cur)?);
            ctx.take_cells(k)?;
            for _ in 0..k {
                cells.column.push(value, None);
            }
        }
        done += k;
    }
    Ok(())
}

fn texts(
    cur: &mut Cursor,
    ctx: &mut Context,
    cells: &mut Cells,
    m: usize,
    inline: bool,
) -> Result<(), OriginError> {
    let mut done = 0;
    while done < m {
        let (distinct, k) = segment(cur, m - done)?;
        if distinct {
            for _ in 0..k {
                let cell = text_cell(cur, inline)?;
                push_text(cells, ctx, &cell, 1)?;
            }
        } else {
            let cell = text_cell(cur, inline)?;
            push_text(cells, ctx, &cell, k)?;
        }
        done += k;
    }
    Ok(())
}

/// Reads the cell block that starts at the cursor.
pub(super) fn read_cells(cur: &mut Cursor, ctx: &mut Context) -> Result<Cells, OriginError> {
    let at = cur.pos();
    let inline = match cur.u8()? {
        0x05 => true,
        0x04 => false,
        _ => return Err(Cursor::unsupported_at(at, "unknown cell block flag")),
    };
    let rows = cur.leb_usize()?;
    let mut cells = Cells {
        column: ColumnBuilder::new(),
        refs: Vec::new(),
    };
    while cells.column.len() < rows {
        let at = cur.pos();
        let room = rows - cells.column.len();
        match *cur.take(2)? {
            [0xFF, 0xFF] => {
                let m = cur.leb_usize()?;
                if m > room {
                    return Err(Cursor::unsupported_at(at, "run overruns the row count"));
                }
                match cur.u8()? {
                    0 => numbers(cur, ctx, &mut cells, m)?,
                    1 => texts(cur, ctx, &mut cells, m, inline)?,
                    _ => return Err(Cursor::unsupported_at(at, "unknown run kind")),
                }
            }
            [0x01, 0x00] => {
                let cell = text_cell(cur, inline)?;
                push_text(&mut cells, ctx, &cell, 1)?;
            }
            [0x00, 0x00] => {
                let value = number_from_bits(value_spec(cur)?);
                ctx.take_cells(1)?;
                cells.column.push(value, None);
            }
            _ => return Err(Cursor::unsupported_at(at, "unknown cell item")),
        }
    }
    Ok(cells)
}

/// The decompressed string table of a column-properties blob, if it has one.
fn string_table(props: &[u8], at: usize) -> Result<Option<Vec<u8>>, OriginError> {
    let Some(key) = props.windows(TABLE_KEY.len()).position(|w| w == TABLE_KEY) else {
        return Ok(None);
    };
    let from = key + TABLE_KEY.len();
    let rest = props.get(from..).unwrap_or(&[]);
    let mut cur = Cursor::new(rest, 0);
    let bad = |what| Cursor::unsupported_at(at + from, what);
    cur.expect(&[0x05, 0x01], "string table marker")
        .map_err(|_| bad("string table marker expected"))?;
    let n = cur.leb_usize().map_err(|_| bad("string table size"))?;
    cur.u8().map_err(|_| bad("string table truncated"))?;
    let body = cur.take(n).map_err(|_| bad("string table truncated"))?;
    let (size, block) = body
        .split_first_chunk::<4>()
        .ok_or_else(|| bad("string table too short"))?;
    let dsz = usize::try_from(u32::from_le_bytes(*size)).map_err(|_| bad("string table size"))?;
    let table = lz4_block(block, dsz).map_err(bad)?;
    if table.get(..4) != Some(&TABLE_MAGIC[..]) || table.len() < TABLE_HEADER {
        return Err(bad("string table magic"));
    }
    Ok(Some(table))
}

/// Fills in the text cells stored by reference from the string table found
/// in the column-properties blob `props` (at file offset `at`).
pub(super) fn resolve_refs(
    cells: &mut Cells,
    ctx: &mut Context,
    props: &[u8],
    at: usize,
) -> Result<(), OriginError> {
    if cells.refs.is_empty() {
        return Ok(());
    }
    let table = string_table(props, at)?.ok_or(OriginError::OpjuUnsupported {
        offset: at,
        what: "text reference without a string table",
    })?;
    let strings = table.get(TABLE_HEADER..).unwrap_or(&[]);
    for &(row, reference) in &cells.refs {
        let tail = strings
            .get(reference - 1..)
            .ok_or(OriginError::OpjuUnsupported {
                offset: at,
                what: "text reference outside the string table",
            })?;
        let s = text(tail);
        if !s.is_empty() {
            ctx.take_text(s.len())?;
            cells.column.set_text(row, s);
        }
    }
    Ok(())
}

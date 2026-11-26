// SPDX-License-Identifier: GPL-3.0-or-later
//
// Original reverse engineering of the `.opju` container (CPYUA, Origin 2018+),
// checked against real files. The FPC parameters, the segment grammar and the
// value tags were first documented by the Apache-2.0 project quantized
// (https://github.com/pquarterman17/quantized, docs/origin_project_format.md,
// (c) 2026 Paige Quarterman); facts only, no code or text is copied. See
// docs/license-and-extensions.md.

//! Reader of the workbooks of an Origin `.opju` file (container `CPYUA`).
//!
//! The `.opju` is the `.opj` object stream re-encoded: the same objects in
//! the same order, but with varints, zero-run-packed headers, LZ4 values and
//! a compact cell encoding. The byte-level pieces are in `opju_codec`, the
//! cell block in `opju_cells`; this module walks the sections:
//!
//! ```text
//! file     = "CPYUA <ver> <build>\n"  blob  "PrvwOPJU"  u64
//!            legacy(global header)  legacy(empty)  datasets  windows  ...
//! datasets = (packed(header) blob cells CE FA blob(properties) 00 00 00)*
//!            LISTEND
//! windows  = (blob  window)*  blob  LISTEND
//! window   = packed(header)  textline(label)  blob(storage)
//!            (blob  layer)*  blob  LISTEND
//! layer    = packed(header)  textline(sheet name)  blob  blob(properties)
//!            annotation* LISTEND  curve* LISTEND  (packed* LISTEND){4}
//! curve    = packed(header)  blob(column properties)
//!            blob(zero-run code of "LongName\r\nUnit\r\nComment\0")  blob
//! annot.   = packed(header) blob block1 block2 block3 00 00
//!            block1 = packed | 00 legacy
//!            block2 = legacy | LZ4 value (0x81) | blob
//!            block3 = legacy | blob
//! ```
//!
//! Annotation header and formulas: in a sheet layer of a book, the header
//! (after unpacking) holds at 0x46, 41 bytes, a name; block 1 holds a
//! NUL-terminated text. When the name is the short name of a column of that
//! sheet the text is the formula of the column (`window::formula_text`
//! refuses blank, over-long and control-character texts; see `mod.rs`).
//! Block 1 is read in both forms. A graph layer has no annotation of
//! interest: its curve records carry at header offsets 0x04 and 0x23 the
//! 1-based ranks of the Y and X datasets, as in the `.opj`.
//!
//! The `u64` after `PrvwOPJU` is an offset or a size that is ignored; when a
//! legacy object does not follow it, `u64` bytes are skipped once and the
//! global header is tried again. A legacy object is chosen over a blob for
//! block 2 and 3 only when byte 4 is `0x0A` and the `u32` size is 0 or is
//! followed by `0x0A`. After the last window come trailing sections that are
//! not needed.
//!
//! The Horiba information tree is in the window's property blob under the
//! key `0x07` + `TREE`, followed by `04 01`, `LEB n`, one byte and `n` bytes
//! (`u32 LE dsz` + LZ4 block): the decompressed text is the
//! `<OriginStorage>` XML that `horiba::info_lines` flattens.
//!
//! Partial read: an error in the dataset section is an error of the read. An
//! error in the window section (an unknown annotation form, for example)
//! keeps the windows read so far and sets `OriginProject::partial`; the
//! books whose window is missing are then built from their datasets alone.
//!
//! Text is UTF-8, read as Windows-1252 when it is not valid UTF-8 (not
//! observed: the files seen are ASCII).
//!
//! Robustness (invariant 10): see `opju_codec`; there is no recursion (an
//! annotation group form is unknown, hence a window-section error), every
//! loop consumes input, and cells are counted against a budget.

use super::dataset::{classify_name, ColumnData, Dataset, DatasetKind, Named};
use super::opju_cells::{read_cells, resolve_refs, Context};
use super::opju_codec::{lz4_block, text, unzero_run, Cursor};
use super::reader::le_u16;
use super::window::{
    curve_from, formula_text, Formula, Layer, Window, ANNOTATION_NAME_AT, ANNOTATION_NAME_LEN,
};
use super::{assemble, check_version, OriginError, OriginProject};

/// The first line of the file has been read; `first` is the offset after it.
pub(super) fn read(bytes: &[u8], first: usize) -> Result<OriginProject, OriginError> {
    let mut cur = Cursor::new(bytes, first);
    cur.blob()?; // preamble
    cur.expect(b"PrvwOPJU", "preview tag expected")?;
    let skip = {
        let raw = cur.take(8)?;
        raw.try_into().map(u64::from_le_bytes).unwrap_or(0)
    };
    let after_tag = cur.pos();
    let global = match cur.legacy() {
        Ok(global) => global,
        Err(first_error) => {
            // The u64 may be the size of a preview that sits before the header.
            cur.set_pos(after_tag);
            let skip = usize::try_from(skip).map_err(|_| first_error.clone())?;
            cur.take(skip).map_err(|_| first_error)?;
            cur.legacy()?
        }
    };
    let version = check_version(global)?;
    if !cur.legacy()?.is_empty() {
        return Err(cur.unsupported("empty object expected after the global header"));
    }
    let mut ctx = Context::new(bytes.len());
    let datasets = read_datasets(&mut cur, &mut ctx)?;
    let mut windows = Vec::new();
    let partial = read_windows(&mut cur, &mut windows).is_err();
    Ok(assemble(version, datasets, windows, partial))
}

fn read_datasets(cur: &mut Cursor, ctx: &mut Context) -> Result<Vec<Dataset>, OriginError> {
    let mut out = Vec::new();
    while !cur.at_list_end() {
        let at = cur.pos();
        let header = cur.packed()?;
        cur.blob()?; // always empty so far
        let mut cells = read_cells(cur, ctx)?;
        cur.expect(&[0xCE, 0xFA], "column properties expected")?;
        let props_at = cur.pos();
        let props = cur.blob()?;
        resolve_refs(&mut cells, ctx, props, props_at)?;
        cur.expect(&[0, 0, 0], "dataset end expected")?;
        let data_type = le_u16(&header, 0x16)
            .ok_or_else(|| Cursor::unsupported_at(at, "short dataset header"))?;
        let name = text(header.get(0x58..).unwrap_or(&[]).get(..25).unwrap_or(&[]));
        let kind = match classify_name(&name, data_type) {
            Named::Column {
                book,
                sheet,
                column,
            } => {
                let (values, texts) = cells.column.finish();
                DatasetKind::Column(ColumnData {
                    book,
                    sheet,
                    column,
                    values,
                    texts,
                })
            }
            Named::Matrix(name) => DatasetKind::Matrix(name),
            Named::Function => DatasetKind::Function,
        };
        out.push(Dataset { kind });
    }
    cur.list_end()?;
    Ok(out)
}

/// Reads the windows into `out`; on an error the windows already pushed stay.
fn read_windows(cur: &mut Cursor, out: &mut Vec<Window>) -> Result<(), OriginError> {
    loop {
        cur.blob()?; // empty before a book; a graph window carries one
        if cur.at_list_end() {
            return cur.list_end();
        }
        let window = read_window(cur)?;
        out.push(window);
    }
}

fn read_window(cur: &mut Cursor) -> Result<Window, OriginError> {
    let header = cur.packed()?;
    let name = text(header.get(2..).unwrap_or(&[]).get(..25).unwrap_or(&[]));
    let label = text(cur.text_line()?);
    let storage = tree_text(cur.blob()?, cur.pos())?;
    let mut layers = Vec::new();
    loop {
        cur.blob()?; // always empty so far
        if cur.at_list_end() {
            cur.list_end()?;
            break;
        }
        layers.push(read_layer(cur)?);
    }
    Ok(Window {
        name,
        label,
        storage,
        layers,
    })
}

fn read_layer(cur: &mut Cursor) -> Result<Layer, OriginError> {
    cur.packed()?; // layer header
    let sheet_name = text(cur.text_line()?);
    cur.blob()?; // always empty so far
    cur.blob()?; // layer properties
    let mut formulas = Vec::new();
    while !cur.at_list_end() {
        read_annotation(cur, &mut formulas)?;
    }
    cur.list_end()?;
    let mut curves = Vec::new();
    while !cur.at_list_end() {
        let header = cur.packed()?;
        cur.blob()?; // column property set
        let packed_text = cur.blob()?;
        let at = cur.pos();
        cur.blob()?; // always empty so far
        let decoded = unzero_run(packed_text).map_err(|what| Cursor::unsupported_at(at, what))?;
        let short = text(header.get(0x12..).unwrap_or(&[]));
        curves.push(curve_from(&header, short, &text(&decoded)));
    }
    cur.list_end()?;
    // Axis breaks, then the x, y and z axis parameters.
    for _ in 0..4 {
        while !cur.at_list_end() {
            cur.packed()?;
        }
        cur.list_end()?;
    }
    Ok(Layer {
        sheet_name,
        curves,
        formulas,
    })
}

/// One annotation; when its header names a column and block 1 holds a usable
/// text, the formula goes to `out`.
fn read_annotation(cur: &mut Cursor, out: &mut Vec<Formula>) -> Result<(), OriginError> {
    let header = cur.packed()?;
    cur.blob()?;
    let block1 = match cur.peek()? {
        0x80 => cur.packed()?,
        0x00 => {
            cur.u8()?;
            cur.legacy()?.to_vec()
        }
        _ => return Err(cur.unsupported("unknown annotation form")),
    };
    let name_bytes = header
        .get(ANNOTATION_NAME_AT..)
        .map(|rest| rest.get(..ANNOTATION_NAME_LEN).unwrap_or(rest));
    let column = name_bytes.map(text).filter(|name| !name.is_empty());
    if let (Some(column), Some(text)) = (column, formula_text(&block1, text)) {
        out.push(Formula { column, text });
    }
    // Block 2 may also be an LZ4 value.
    if cur.peek()? == 0x81 {
        cur.lz4_value()?;
    } else {
        legacy_or_blob(cur)?;
    }
    legacy_or_blob(cur)?;
    cur.expect(&[0, 0], "annotation end expected")
}

/// A legacy object when the bytes look like one, else a blob.
fn legacy_or_blob(cur: &mut Cursor) -> Result<(), OriginError> {
    if looks_like_legacy(cur) {
        cur.legacy()?;
    } else {
        cur.blob()?;
    }
    Ok(())
}

/// Byte 4 is `0x0A` and the `u32` size is 0 or is followed by `0x0A`.
fn looks_like_legacy(cur: &Cursor) -> bool {
    let Some(head) = cur.peek_bytes(5) else {
        return false;
    };
    let [a, b, c, d, delimiter] = *head else {
        return false;
    };
    if delimiter != 0x0A {
        return false;
    }
    let size = u32::from_le_bytes([a, b, c, d]) as usize;
    size == 0 || cur.peek_at(5 + size) == Some(0x0A)
}

/// The decompressed Horiba trees found in a window's property blob, joined.
/// `at` is the offset just after the blob, for error reports.
fn tree_text(storage: &[u8], at: usize) -> Result<String, OriginError> {
    const KEY: &[u8] = b"\x07TREE";
    let mut out = String::new();
    let mut from = 0;
    while let Some(found) = storage
        .get(from..)
        .and_then(|rest| rest.windows(KEY.len()).position(|w| w == KEY))
    {
        let start = from + found + KEY.len();
        from = start;
        let rest = storage.get(start..).unwrap_or(&[]);
        if !rest.starts_with(&[0x04, 0x01]) {
            continue; // not a tree record
        }
        let bad = |what| Cursor::unsupported_at(at, what);
        let mut cur = Cursor::new(rest, 2);
        let n = cur.leb_usize().map_err(|_| bad("tree size"))?;
        cur.u8().map_err(|_| bad("tree truncated"))?;
        let body = cur.take(n).map_err(|_| bad("tree truncated"))?;
        let (size, block) = body
            .split_first_chunk::<4>()
            .ok_or_else(|| bad("tree too short"))?;
        let dsz = usize::try_from(u32::from_le_bytes(*size)).map_err(|_| bad("tree size"))?;
        let xml = lz4_block(block, dsz).map_err(bad)?;
        out.push_str(&text(&xml));
    }
    Ok(out)
}

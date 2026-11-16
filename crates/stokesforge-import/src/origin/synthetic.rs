// SPDX-License-Identifier: GPL-3.0-or-later

//! Test generator: writes a minimal valid `.opj` ([`write_project`]) or
//! `.opju` ([`write_opju`]) from a description, so the readers are tested
//! without a real (private, copyrighted) Origin file. The `.opj` framing and
//! offsets are those the reader takes from liborigin 3.0.3; the `.opju`
//! writer follows the grammar in the docs of `opju.rs` and `opju_cells.rs`
//! (its FPC encoder and zero-run encoder are written independently of the
//! decoders). Not part of the product API.

use super::OriginDesignation;

/// First line of every generated file.
pub const SIGNATURE: &[u8] = b"CPYA 4.3268 195 W64 #\n";
/// Size of the generated global header.
pub const GLOBAL_HEADER_LEN: usize = 0x30;
/// Offset of the size field of the first dataset header (or of the dataset
/// list's end mark when there is no dataset).
pub const FIRST_DATASET_OFFSET: usize = SIGNATURE.len() + 5 + GLOBAL_HEADER_LEN + 1 + 5;

/// How the cells of a column are stored.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cells {
    /// "Text & numeric": a flag byte, one more byte, then an f64 or the text.
    Mixed,
    /// Plain f64 cells (8 bytes).
    Numeric,
    /// Text only, NUL-padded.
    Text,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Cell {
    Number(f64),
    /// Origin's missing-value literal.
    Missing,
    Text(String),
}

/// How the `.opju` writer cuts the numeric cells of a column into items.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpjuNumbers {
    /// One run: groups of 3 or more equal values as repeat segments, the
    /// others in FPC segments.
    Auto,
    /// One run of repeat segments, one per group of equal values (even 1).
    Runs,
    /// One `00 00 value` item per cell.
    Single,
}

#[derive(Debug, Clone)]
pub struct Column {
    pub short_name: String,
    pub designation: OriginDesignation,
    pub long_name: String,
    pub unit: String,
    pub comment: String,
    pub cells: Vec<Cell>,
    pub storage: Cells,
    /// `.opju` only: how the numeric (and text) cells are cut into items.
    pub opju_numbers: OpjuNumbers,
    /// `.opju` only: text cells go to the string table (flag 0x04) instead
    /// of being written inline (flag 0x05).
    pub opju_text_by_reference: bool,
}

impl Column {
    pub fn numbers(short_name: &str, designation: OriginDesignation, values: &[f64]) -> Self {
        Self {
            short_name: short_name.into(),
            designation,
            long_name: String::new(),
            unit: String::new(),
            comment: String::new(),
            cells: values.iter().map(|&v| Cell::Number(v)).collect(),
            storage: Cells::Mixed,
            opju_numbers: OpjuNumbers::Auto,
            opju_text_by_reference: false,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Sheet {
    pub name: String,
    pub columns: Vec<Column>,
}

#[derive(Debug, Clone)]
pub struct Book {
    pub short_name: String,
    pub long_name: String,
    pub sheets: Vec<Sheet>,
    /// The `<HJYInfo>...` text appended to the label in an `OriginStorage` block.
    pub horiba_xml: Option<String>,
}

#[derive(Debug, Clone)]
pub struct Project {
    pub version: f64,
    pub books: Vec<Book>,
    /// Matrix windows (each with one dataset and no layer).
    pub matrices: Vec<String>,
    /// Graph windows (no dataset).
    pub graphs: Vec<String>,
    /// Number of annotation groups nested in the first layer of the first book.
    pub annotation_nesting: usize,
    /// `.opju` only: annotations of each block form in the first layer of
    /// the first book.
    pub opju_annotations: bool,
    /// `.opju` only: an annotation of an unknown form in the first layer of
    /// this book (the window section cannot be read from there).
    pub opju_bad_annotation: Option<usize>,
    /// `.opju` only: bytes of preview between the `PrvwOPJU` size field and
    /// the global header; the size field holds their count.
    pub opju_preview_len: usize,
    /// `.opju` only: text is written as UTF-8 (else as Windows-1252).
    pub opju_utf8: bool,
}

impl Default for Project {
    fn default() -> Self {
        Self {
            version: 9.5,
            books: Vec::new(),
            matrices: Vec::new(),
            graphs: Vec::new(),
            annotation_nesting: 0,
            opju_annotations: false,
            opju_bad_annotation: None,
            opju_preview_len: 0,
            opju_utf8: false,
        }
    }
}

/// Latin-1 plus the euro sign, enough for the test texts (Windows-1252).
fn encode(text: &str) -> Vec<u8> {
    text.chars()
        .map(|c| match c {
            '€' => 0x80,
            c if (c as u32) < 0x100 => c as u32 as u8,
            _ => b'?',
        })
        .collect()
}

fn put_size(out: &mut Vec<u8>, size: usize) {
    out.extend_from_slice(&(size as u32).to_le_bytes());
    out.push(0x0A);
}

fn put_object(out: &mut Vec<u8>, bytes: &[u8]) {
    put_size(out, bytes.len());
    if !bytes.is_empty() {
        out.extend_from_slice(bytes);
        out.push(0x0A);
    }
}

fn put_end(out: &mut Vec<u8>) {
    put_size(out, 0);
}

fn put_at(buf: &mut [u8], off: usize, bytes: &[u8]) {
    buf[off..off + bytes.len()].copy_from_slice(bytes);
}

fn column_code(designation: OriginDesignation) -> u8 {
    match designation {
        OriginDesignation::X => 3,
        OriginDesignation::Y => 0,
        OriginDesignation::Z => 5,
        OriginDesignation::XError => 6,
        OriginDesignation::YError => 2,
        OriginDesignation::Label => 4,
        OriginDesignation::Disregard => 1,
    }
}

const MISSING: f64 = -1.23456789e-300;

fn put_dataset(
    out: &mut Vec<u8>,
    name: &str,
    data_type: u16,
    value_size: usize,
    rows: usize,
    data: &[u8],
) {
    let mut header = vec![0u8; 0x80];
    put_at(&mut header, 0x16, &data_type.to_le_bytes());
    put_at(&mut header, 0x19, &(rows as i32).to_le_bytes());
    header[0x3D] = value_size as u8;
    let name = encode(name);
    put_at(&mut header, 0x58, &name[..name.len().min(24)]);
    put_object(out, &header);
    put_object(out, data);
    put_end(out); // mask
}

fn put_column(out: &mut Vec<u8>, dataset_name: &str, column: &Column) {
    let (data_type, value_size, data) = match column.storage {
        Cells::Numeric => {
            let mut data = Vec::new();
            for cell in &column.cells {
                let v = match cell {
                    Cell::Number(v) => *v,
                    Cell::Missing | Cell::Text(_) => MISSING,
                };
                data.extend_from_slice(&v.to_le_bytes());
            }
            (0x6001, 8, data)
        }
        Cells::Mixed => {
            let texts: Vec<Vec<u8>> = column
                .cells
                .iter()
                .map(|c| match c {
                    Cell::Text(t) => encode(t),
                    _ => Vec::new(),
                })
                .collect();
            let size = texts.iter().map(|t| t.len() + 3).max().unwrap_or(0).max(10);
            let mut data = Vec::new();
            for (cell, text) in column.cells.iter().zip(&texts) {
                let mut raw = vec![0u8; size];
                match cell {
                    Cell::Text(_) => {
                        raw[0] = 1;
                        put_at(&mut raw, 2, text);
                    }
                    Cell::Number(v) => put_at(&mut raw, 2, &v.to_le_bytes()),
                    Cell::Missing => put_at(&mut raw, 2, &MISSING.to_le_bytes()),
                }
                data.extend_from_slice(&raw);
            }
            (0x0100, size, data)
        }
        Cells::Text => {
            let texts: Vec<Vec<u8>> = column
                .cells
                .iter()
                .map(|c| match c {
                    Cell::Text(t) => encode(t),
                    _ => Vec::new(),
                })
                .collect();
            let size = texts.iter().map(|t| t.len() + 1).max().unwrap_or(1).max(2);
            let mut data = Vec::new();
            for text in &texts {
                let mut raw = vec![0u8; size];
                put_at(&mut raw, 0, text);
                data.extend_from_slice(&raw);
            }
            (0x0000, size, data)
        }
    };
    put_dataset(
        out,
        dataset_name,
        data_type,
        value_size,
        column.cells.len(),
        &data,
    );
}

fn window_header(name: &str, label_and_storage: &str) -> Vec<u8> {
    let tail = encode(label_and_storage);
    let mut header = vec![0u8; 0xC3 + tail.len() + 1];
    let name = encode(name);
    put_at(&mut header, 0x02, &name[..name.len().min(24)]);
    put_at(&mut header, 0xC3, &tail);
    header
}

fn layer_header(sheet_name: &str) -> Vec<u8> {
    let mut header = vec![0u8; 0xD2 + 32];
    let name = encode(sheet_name);
    put_at(&mut header, 0xD2, &name[..name.len().min(31)]);
    header
}

fn put_curve(out: &mut Vec<u8>, column: &Column) {
    let name = encode(&column.short_name);
    let mut header = vec![0u8; 0x12 + name.len() + 1];
    header[0x11] = column_code(column.designation);
    put_at(&mut header, 0x12, &name);
    put_object(out, &header);
    let mut text = format!("{}\r\n{}", column.long_name, column.unit);
    if !column.comment.is_empty() {
        text.push_str("\r\n");
        text.push_str(&column.comment);
    }
    let mut data = encode(&text);
    data.push(0);
    put_object(out, &data);
}

/// One annotation group holding `depth - 1` more, innermost empty.
fn put_annotation_group(out: &mut Vec<u8>, depth: usize) {
    if depth == 0 {
        return;
    }
    put_object(out, &[0u8; 8]); // annotation header
    put_object(out, &[0u8; 0x0A]); // first block, size 0x0A: a group
    put_size(out, 4); // second block announces the nested list
    put_annotation_group(out, depth - 1);
    put_end(out); // end of the nested list
    put_end(out); // third block (empty)
}

fn put_layer(out: &mut Vec<u8>, sheet: Option<&Sheet>, nesting: usize) {
    put_object(out, &layer_header(sheet.map_or("", |s| s.name.as_str())));
    put_annotation_group(out, nesting);
    put_end(out); // annotations
    if let Some(sheet) = sheet {
        for column in &sheet.columns {
            put_curve(out, column);
        }
    }
    put_end(out); // curves
    put_end(out); // axis breaks
    for _axis in 0..3 {
        put_end(out); // axis parameters x, y, z
    }
}

/// The bytes of the `.opj` that describes `project`. The file ends with the
/// end mark of the window list, so every shorter prefix is truncated.
pub fn write_project(project: &Project) -> Vec<u8> {
    let mut out = SIGNATURE.to_vec();
    let mut global = vec![0u8; GLOBAL_HEADER_LEN];
    put_at(&mut global, 0x1B, &project.version.to_le_bytes());
    put_object(&mut out, &global);
    put_end(&mut out);

    for book in &project.books {
        for (n, sheet) in book.sheets.iter().enumerate() {
            for column in &sheet.columns {
                let suffix = if n == 0 {
                    String::new()
                } else {
                    format!("@{}", n + 1)
                };
                let name = format!("{}_{}{}", book.short_name, column.short_name, suffix);
                put_column(&mut out, &name, column);
            }
        }
    }
    for matrix in &project.matrices {
        let data: Vec<u8> = [1.0f64, 2.0, 3.0, 4.0]
            .iter()
            .flat_map(|v| v.to_le_bytes())
            .collect();
        put_dataset(&mut out, matrix, 0x6001, 8, 4, &data);
    }
    put_end(&mut out);

    let mut nesting = project.annotation_nesting;
    for book in &project.books {
        let mut label = book.long_name.clone();
        if let Some(xml) = &book.horiba_xml {
            label.push_str("@${[0|4|TREE|1|2]}<OriginStorage>");
            label.push_str(xml);
            label.push_str("</OriginStorage>");
        }
        put_object(&mut out, &window_header(&book.short_name, &label));
        for sheet in &book.sheets {
            put_layer(&mut out, Some(sheet), std::mem::take(&mut nesting));
        }
        put_end(&mut out);
    }
    for name in project.matrices.iter().chain(&project.graphs) {
        put_object(&mut out, &window_header(name, ""));
        put_layer(&mut out, None, 0);
        put_end(&mut out);
    }
    put_end(&mut out);
    out
}

// ---- The `.opju` writer.

/// The signature line of every generated `.opju`.
pub const OPJU_SIGNATURE: &[u8] = b"CPYUA 4.3268 195\n";
/// The key that precedes the string table in a column-properties blob.
pub const STRING_TABLE_KEY: &[u8] = b"\x23_MSER_STRINGS_PSET";
/// The six bytes that close every list.
pub const LIST_END: [u8; 6] = [0, 0, 0, 0, 0, 0x0A];
/// Length of the trailing bytes [`write_opju`] leaves after the window
/// section.
pub const OPJU_TRAILER_LEN: usize = 17;

/// Appends a LEB128 varint.
pub fn put_leb(out: &mut Vec<u8>, mut v: u64) {
    loop {
        let low = (v & 0x7F) as u8;
        v >>= 7;
        if v == 0 {
            out.push(low);
            return;
        }
        out.push(low | 0x80);
    }
}

/// Appends a zigzag varint.
pub fn put_zigzag(out: &mut Vec<u8>, v: i64) {
    put_leb(out, ((v << 1) ^ (v >> 63)) as u64);
}

/// Zero-run code of `src`: literals, zero runs (`0x80 | n - 3`) and fill
/// runs (`0xC0 | n - 3`, then the byte), runs of 3 to 66 bytes.
pub fn zero_run(src: &[u8]) -> Vec<u8> {
    fn flush(out: &mut Vec<u8>, literal: &mut Vec<u8>) {
        if !literal.is_empty() {
            out.push(literal.len() as u8);
            out.append(literal);
        }
    }
    let mut out = Vec::new();
    let mut literal = Vec::new();
    let mut i = 0;
    while i < src.len() {
        let b = src[i];
        let mut run = 1;
        while i + run < src.len() && src[i + run] == b && run < 66 {
            run += 1;
        }
        if run >= 3 {
            flush(&mut out, &mut literal);
            if b == 0 {
                out.push(0x80 | (run - 3) as u8);
            } else {
                out.push(0xC0 | (run - 3) as u8);
                out.push(b);
            }
            i += run;
        } else {
            literal.push(b);
            i += 1;
            if literal.len() == 127 {
                flush(&mut out, &mut literal);
            }
        }
    }
    flush(&mut out, &mut literal);
    out
}

/// An LZ4 block made of one literal sequence (valid LZ4).
pub fn lz4_literal(src: &[u8]) -> Vec<u8> {
    let mut out = vec![(src.len().min(15) as u8) << 4];
    if src.len() >= 15 {
        let mut rest = src.len() - 15;
        while rest >= 255 {
            out.push(255);
            rest -= 255;
        }
        out.push(rest as u8);
    }
    out.extend_from_slice(src);
    out
}

/// `0x80`, `LEB L`, the zero-run code of `bytes`.
pub fn put_packed(out: &mut Vec<u8>, bytes: &[u8]) {
    put_packed_code(out, &zero_run(bytes));
}

/// `0x80`, `LEB L`, `code` as given.
pub fn put_packed_code(out: &mut Vec<u8>, code: &[u8]) {
    out.push(0x80);
    put_leb(out, code.len() as u64);
    out.extend_from_slice(code);
}

/// `LEB n`, the bytes, `0x0A` when not empty.
pub fn put_blob(out: &mut Vec<u8>, bytes: &[u8]) {
    put_leb(out, bytes.len() as u64);
    if !bytes.is_empty() {
        out.extend_from_slice(bytes);
        out.push(0x0A);
    }
}

fn put_text_line(out: &mut Vec<u8>, bytes: &[u8]) {
    put_leb(out, bytes.len() as u64);
    out.extend_from_slice(bytes);
    out.push(0x0A);
}

fn put_lz4_value(out: &mut Vec<u8>, bytes: &[u8]) {
    out.push(0x81);
    put_leb(out, bytes.len() as u64);
    let block = lz4_literal(bytes);
    put_leb(out, block.len() as u64);
    out.extend_from_slice(&block);
}

/// `u32 dsz` + an LZ4 block, the payload of a compressed property.
pub fn dsz_and_block(bytes: &[u8]) -> Vec<u8> {
    let mut out = (bytes.len() as u32).to_le_bytes().to_vec();
    out.extend_from_slice(&lz4_literal(bytes));
    out
}

fn encode_text(text: &str, utf8: bool) -> Vec<u8> {
    if utf8 {
        text.as_bytes().to_vec()
    } else {
        encode(text)
    }
}

/// The tag and bytes of a value spec holding the `f64` bit pattern `bits`.
fn value_spec(bits: u64) -> Vec<u8> {
    if bits == 0 {
        return vec![0x64];
    }
    if bits == MISSING.to_bits() {
        return vec![0x65];
    }
    // The fewest top bytes that hold every set bit.
    let k = (1..=8usize)
        .find(|&k| bits.trailing_zeros() as usize >= 64 - 8 * k)
        .unwrap_or(8);
    let mut out = vec![(8 + 9 * k) as u8];
    out.extend_from_slice(&(bits >> (64 - 8 * k)).to_le_bytes()[..k]);
    out
}

/// FPC encoding of `values` (a fresh state): exact DFCM prediction, exact
/// FCM prediction, else the FCM residual at its narrowest width.
pub fn fpc_encode(values: &[u64]) -> Vec<u8> {
    let (mut fcm, mut dfcm) = (vec![0u64; 4096], vec![0u64; 4096]);
    let (mut fh, mut dh, mut last) = (0usize, 0usize, 0u64);
    let mut nibbles: Vec<(u8, Vec<u8>)> = Vec::new();
    for &v in values {
        let by_stride = last.wrapping_add(dfcm[dh]);
        let by_value = fcm[fh];
        let (code, residual): (u8, Vec<u8>) = if v == by_stride {
            (0x8, Vec::new())
        } else if v == by_value {
            (0x0, Vec::new())
        } else {
            let r = v ^ by_value;
            let width = [1usize, 2, 3, 5, 6, 7, 8]
                .into_iter()
                .find(|&w| w == 8 || r >> (8 * w) == 0)
                .unwrap_or(8);
            let code = if width < 4 { width } else { width - 1 } as u8;
            (code, r.to_le_bytes()[..width].to_vec())
        };
        nibbles.push((code, residual));
        let stride = v.wrapping_sub(last);
        fcm[fh] = v;
        dfcm[dh] = stride;
        fh = ((fh << 6) ^ (v >> 48) as usize) & 0xFFF;
        dh = ((dh << 2) ^ (stride >> 40) as usize) & 0xFFF;
        last = v;
    }
    let mut out = Vec::new();
    for pair in nibbles.chunks(2) {
        out.push(pair[0].0 | pair.get(1).map_or(0, |p| p.0 << 4));
        for (_, residual) in pair {
            out.extend_from_slice(residual);
        }
    }
    out
}

/// One cell of a generated column, as the `.opju` stores it.
#[derive(Clone, PartialEq)]
enum OpjuCell {
    Number(u64),
    Text(String),
}

fn opju_cells(column: &Column) -> Vec<OpjuCell> {
    column
        .cells
        .iter()
        .map(|cell| match (cell, column.storage) {
            (Cell::Number(v), Cells::Mixed | Cells::Numeric) => OpjuCell::Number(v.to_bits()),
            (Cell::Text(t), Cells::Mixed | Cells::Text) => OpjuCell::Text(t.clone()),
            _ => OpjuCell::Number(MISSING.to_bits()),
        })
        .collect()
}

/// Segments of equal items: `(count, index of the first)`.
fn groups<T: PartialEq>(items: &[T]) -> Vec<(usize, usize)> {
    let mut out: Vec<(usize, usize)> = Vec::new();
    for (i, item) in items.iter().enumerate() {
        match out.last_mut() {
            Some((count, first)) if items[*first] == *item => *count += 1,
            _ => out.push((1, i)),
        }
    }
    out
}

fn put_numbers(out: &mut Vec<u8>, bits: &[u64], mode: OpjuNumbers) {
    if mode == OpjuNumbers::Single {
        for &b in bits {
            out.extend_from_slice(&[0, 0]);
            out.extend_from_slice(&value_spec(b));
        }
        return;
    }
    out.extend_from_slice(&[0xFF, 0xFF]);
    put_leb(out, bits.len() as u64);
    out.push(0);
    let mut distinct: Vec<u64> = Vec::new();
    let flush = |out: &mut Vec<u8>, distinct: &mut Vec<u64>| {
        if !distinct.is_empty() {
            put_zigzag(out, -(distinct.len() as i64));
            out.push(0x0C);
            out.extend_from_slice(&fpc_encode(distinct));
            distinct.clear();
        }
    };
    for (count, first) in groups(bits) {
        if mode == OpjuNumbers::Runs || count >= 3 {
            flush(out, &mut distinct);
            put_zigzag(out, count as i64);
            out.extend_from_slice(&value_spec(bits[first]));
        } else {
            distinct.extend(std::iter::repeat_n(bits[first], count));
        }
    }
    flush(out, &mut distinct);
}

struct TextWriter<'a> {
    by_reference: bool,
    utf8: bool,
    table: &'a mut Vec<String>,
}

impl TextWriter<'_> {
    fn cell(&mut self, out: &mut Vec<u8>, text: &str) {
        if self.by_reference {
            let reference = if text.is_empty() {
                0
            } else {
                let mut offset = 0;
                let mut found = None;
                for known in self.table.iter() {
                    if known == text {
                        found = Some(offset);
                        break;
                    }
                    offset += known.len() + 1;
                }
                let offset = found.unwrap_or_else(|| {
                    self.table.push(text.to_string());
                    offset
                });
                offset + 1
            };
            put_leb(out, reference as u64);
        } else {
            let bytes = encode_text(text, self.utf8);
            put_leb(out, bytes.len() as u64);
            out.extend_from_slice(&bytes);
        }
    }
}

fn put_texts(out: &mut Vec<u8>, texts: &[String], mode: OpjuNumbers, writer: &mut TextWriter) {
    if mode == OpjuNumbers::Single {
        for t in texts {
            out.extend_from_slice(&[1, 0]);
            writer.cell(out, t);
        }
        return;
    }
    out.extend_from_slice(&[0xFF, 0xFF]);
    put_leb(out, texts.len() as u64);
    out.push(1);
    let mut distinct: Vec<&String> = Vec::new();
    let flush = |out: &mut Vec<u8>, distinct: &mut Vec<&String>, writer: &mut TextWriter| {
        if !distinct.is_empty() {
            put_zigzag(out, -(distinct.len() as i64));
            for t in distinct.drain(..) {
                writer.cell(out, t);
            }
        }
    };
    for (count, first) in groups(texts) {
        if mode == OpjuNumbers::Runs || count >= 2 {
            flush(out, &mut distinct, writer);
            put_zigzag(out, count as i64);
            writer.cell(out, &texts[first]);
        } else {
            distinct.push(&texts[first]);
        }
    }
    flush(out, &mut distinct, writer);
}

/// The cell block of a column and its string table (empty when inline).
fn opju_cell_block(column: &Column, utf8: bool) -> (Vec<u8>, Vec<String>) {
    let cells = opju_cells(column);
    let has_text = cells
        .iter()
        .any(|c| matches!(c, OpjuCell::Text(t) if !t.is_empty()));
    let by_reference = column.opju_text_by_reference && has_text;
    let mut table = Vec::new();
    let mut out = vec![if by_reference { 4 } else { 5 }];
    put_leb(&mut out, cells.len() as u64);
    let mut writer = TextWriter {
        by_reference,
        utf8,
        table: &mut table,
    };
    let mut i = 0;
    while i < cells.len() {
        let numeric = matches!(cells[i], OpjuCell::Number(_));
        let end = cells[i..]
            .iter()
            .position(|c| matches!(c, OpjuCell::Number(_)) != numeric)
            .map_or(cells.len(), |n| i + n);
        if numeric {
            let bits: Vec<u64> = cells[i..end]
                .iter()
                .filter_map(|c| match c {
                    OpjuCell::Number(b) => Some(*b),
                    OpjuCell::Text(_) => None,
                })
                .collect();
            put_numbers(&mut out, &bits, column.opju_numbers);
        } else {
            let texts: Vec<String> = cells[i..end]
                .iter()
                .filter_map(|c| match c {
                    OpjuCell::Text(t) => Some(t.clone()),
                    OpjuCell::Number(_) => None,
                })
                .collect();
            put_texts(&mut out, &texts, column.opju_numbers, &mut writer);
        }
        i = end;
    }
    (out, table)
}

/// The column-properties blob: the string table among other bytes.
fn opju_column_properties(table: &[String]) -> Vec<u8> {
    if table.is_empty() {
        return vec![0x01, 0x02, 0x03];
    }
    let mut strings = vec![0x77, 0x11, 0x11, 0x11];
    strings.extend_from_slice(&[0u8; 20]);
    for s in table {
        strings.extend_from_slice(s.as_bytes());
        strings.push(0);
    }
    let payload = dsz_and_block(&strings);
    let mut props = vec![0x00, 0x00];
    props.extend_from_slice(STRING_TABLE_KEY);
    props.extend_from_slice(&[0x05, 0x01]);
    put_leb(&mut props, payload.len() as u64);
    props.push(0x00);
    props.extend_from_slice(&payload);
    props.extend_from_slice(&[0x00, 0x00, 0x07]);
    props
}

/// An `.opju` dataset: packed header, empty blob, `cells`, `CE FA`, the
/// column-properties blob and three zero bytes.
pub fn put_opju_dataset(out: &mut Vec<u8>, header_code: &[u8], cells: &[u8], props: &[u8]) {
    put_packed_code(out, header_code);
    out.push(0);
    out.extend_from_slice(cells);
    out.extend_from_slice(&[0xCE, 0xFA]);
    put_blob(out, props);
    out.extend_from_slice(&[0, 0, 0]);
}

fn opju_dataset_header(name: &str, data_type: u16) -> Vec<u8> {
    let mut header = vec![0u8; 147];
    put_at(&mut header, 0x16, &data_type.to_le_bytes());
    let name = encode(name);
    put_at(&mut header, 0x58, &name[..name.len().min(24)]);
    header
}

fn put_opju_column(out: &mut Vec<u8>, dataset_name: &str, column: &Column, utf8: bool) {
    let (cells, table) = opju_cell_block(column, utf8);
    let header = opju_dataset_header(dataset_name, 0x6001);
    put_opju_dataset(
        out,
        &zero_run(&header),
        &cells,
        &opju_column_properties(&table),
    );
}

fn opju_window_header(name: &str) -> Vec<u8> {
    let mut header = vec![0u8; 0xC3];
    let name = encode(name);
    put_at(&mut header, 0x02, &name[..name.len().min(24)]);
    header
}

fn put_opju_annotation_forms(out: &mut Vec<u8>) {
    // Packed block 1, LZ4 value block 2, blob block 3.
    put_packed(out, &[1u8; 8]);
    put_blob(out, &[]);
    put_packed(out, &[5, 0, 0, 0, 0, 7]);
    put_lz4_value(out, b"lz4 block");
    put_blob(out, b"abc");
    out.extend_from_slice(&[0, 0]);
    // `00` + legacy block 1, legacy block 2, empty legacy block 3.
    put_packed(out, &[2u8; 8]);
    put_blob(out, &[]);
    out.push(0);
    put_object(out, &[1, 2, 3]);
    put_object(out, b"xy");
    put_object(out, &[]);
    out.extend_from_slice(&[0, 0]);
    // A blob that begins like a legacy object (byte 4 is 0x0A).
    put_packed(out, &[3u8; 8]);
    put_blob(out, b"t");
    put_packed(out, &[5, 0, 0, 0, 0, 7]);
    put_blob(out, &[0x11, 0x22, 0x33]);
    put_blob(out, &[]);
    out.extend_from_slice(&[0, 0]);
}

#[derive(Clone, Copy)]
enum Annotations {
    None,
    Forms,
    Bad,
}

fn put_opju_layer(out: &mut Vec<u8>, sheet: Option<&Sheet>, utf8: bool, annotations: Annotations) {
    let mut header = vec![0u8; 0x40];
    header[0x20..0x28].fill(0xFF);
    put_packed(out, &header);
    put_text_line(
        out,
        &encode_text(sheet.map_or("", |s| s.name.as_str()), utf8),
    );
    out.push(0); // always empty so far
    put_blob(out, &[0x42, 0x42, 0x42]); // layer properties
    match annotations {
        Annotations::None => {}
        Annotations::Forms => put_opju_annotation_forms(out),
        Annotations::Bad => {
            put_packed(out, &[9u8; 8]);
            put_blob(out, &[]);
            out.push(0x42); // no such annotation form
        }
    }
    out.extend_from_slice(&LIST_END);
    if let Some(sheet) = sheet {
        for column in &sheet.columns {
            let mut header = vec![0u8; 0x12];
            header[0x11] = column_code(column.designation);
            header.extend_from_slice(&encode_text(&column.short_name, utf8));
            put_packed(out, &header);
            put_blob(out, &[9, 9]); // column property set
            let mut text = format!("{}\r\n{}", column.long_name, column.unit);
            if !column.comment.is_empty() {
                text.push_str("\r\n");
                text.push_str(&column.comment);
            }
            let mut bytes = encode_text(&text, utf8);
            bytes.push(0);
            put_blob(out, &zero_run(&bytes));
            out.push(0); // always empty so far
        }
    }
    out.extend_from_slice(&LIST_END);
    out.extend_from_slice(&LIST_END); // axis breaks
    put_packed(out, &[1, 2, 3]); // one x axis record
    out.extend_from_slice(&LIST_END);
    out.extend_from_slice(&LIST_END); // y
    out.extend_from_slice(&LIST_END); // z
}

fn put_opju_preamble(out: &mut Vec<u8>, version: f64, preview_len: usize) {
    out.extend_from_slice(OPJU_SIGNATURE);
    put_blob(out, &[0x11; 21]);
    out.extend_from_slice(b"PrvwOPJU");
    out.extend_from_slice(&(preview_len as u64).to_le_bytes());
    out.extend(std::iter::repeat_n(0xFFu8, preview_len));
    let mut global = vec![0u8; GLOBAL_HEADER_LEN];
    put_at(&mut global, 0x1B, &version.to_le_bytes());
    put_object(out, &global);
    put_end(out);
}

/// The bytes of the `.opju` that describes `project` (the same `Project` as
/// [`write_project`] takes). The window section ends with its list end,
/// followed by [`OPJU_TRAILER_LEN`] trailing bytes the reader does not need.
pub fn write_opju(project: &Project) -> Vec<u8> {
    let utf8 = project.opju_utf8;
    let mut out = Vec::new();
    put_opju_preamble(&mut out, project.version, project.opju_preview_len);
    for book in &project.books {
        for (n, sheet) in book.sheets.iter().enumerate() {
            for column in &sheet.columns {
                let suffix = if n == 0 {
                    String::new()
                } else {
                    format!("@{}", n + 1)
                };
                let name = format!("{}_{}{}", book.short_name, column.short_name, suffix);
                put_opju_column(&mut out, &name, column, utf8);
            }
        }
    }
    for matrix in &project.matrices {
        let column = Column::numbers("M", OriginDesignation::Y, &[1.0, 2.0, 3.0, 4.0]);
        let (cells, _) = opju_cell_block(&column, utf8);
        let header = opju_dataset_header(matrix, 0x6001);
        put_opju_dataset(&mut out, &zero_run(&header), &cells, &[]);
    }
    out.extend_from_slice(&LIST_END);

    for (index, book) in project.books.iter().enumerate() {
        out.push(0); // a book carries no blob here
        put_packed(&mut out, &opju_window_header(&book.short_name));
        put_text_line(&mut out, &encode_text(&book.long_name, utf8));
        let storage = match &book.horiba_xml {
            Some(xml) => {
                let tree = format!("<OriginStorage>{xml}</OriginStorage>");
                let payload = dsz_and_block(tree.as_bytes());
                let mut storage = vec![0x01, 0x02];
                storage.extend_from_slice(b"\x07TREE\x04\x01");
                put_leb(&mut storage, payload.len() as u64);
                storage.push(0x02);
                storage.extend_from_slice(&payload);
                storage.extend_from_slice(&[0x03, 0x04]);
                storage
            }
            None => vec![0x01, 0x02],
        };
        put_blob(&mut out, &storage);
        for (n, sheet) in book.sheets.iter().enumerate() {
            out.push(0);
            let annotations = match (index, n) {
                (i, 0) if project.opju_bad_annotation == Some(i) => Annotations::Bad,
                (0, 0) if project.opju_annotations => Annotations::Forms,
                _ => Annotations::None,
            };
            put_opju_layer(&mut out, Some(sheet), utf8, annotations);
        }
        out.push(0);
        out.extend_from_slice(&LIST_END);
    }
    for name in project.matrices.iter().chain(&project.graphs) {
        put_blob(&mut out, &[1, 2, 3, 4]); // a graph window carries a blob
        put_packed(&mut out, &opju_window_header(name));
        put_text_line(&mut out, b"");
        put_blob(&mut out, &[0x01, 0x02]);
        out.push(0);
        put_opju_layer(&mut out, None, utf8, Annotations::None);
        out.push(0);
        out.extend_from_slice(&LIST_END);
    }
    out.push(0);
    out.extend_from_slice(&LIST_END);
    out.extend_from_slice(b"TRAILING SECTIONS");
    out
}

/// A minimal `.opju` with one dataset whose packed header code, cell block
/// (from the flag byte) and column-properties blob are given verbatim; the
/// window section is empty. For hand-made defects and bombs.
pub fn write_opju_with_dataset(header_code: &[u8], cells: &[u8], props: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    put_opju_preamble(&mut out, 9.5, 0);
    put_opju_dataset(&mut out, header_code, cells, props);
    out.extend_from_slice(&LIST_END);
    out.push(0);
    out.extend_from_slice(&LIST_END);
    out
}

/// The zero-run code of a plain dataset header named `name`.
pub fn opju_header_code(name: &str) -> Vec<u8> {
    zero_run(&opju_dataset_header(name, 0x6001))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_dataset_offset_points_at_the_first_dataset_size() {
        let bytes = write_project(&Project {
            books: vec![Book {
                short_name: "B".into(),
                long_name: String::new(),
                sheets: vec![Sheet {
                    name: "S".into(),
                    columns: vec![Column::numbers("A", OriginDesignation::Y, &[1.0])],
                }],
                horiba_xml: None,
            }],
            ..Project::default()
        });
        assert_eq!(&bytes[FIRST_DATASET_OFFSET..][..4], &[0x80, 0, 0, 0]);
    }

    #[test]
    fn committed_opju_sample_is_the_generator_output() {
        let committed = std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../test/import/origin/synthetic.opju"
        ))
        .expect("test/import/origin/synthetic.opju");
        assert_eq!(committed, write_opju(&sample()));
    }

    #[test]
    fn committed_sample_is_the_generator_output() {
        let committed = std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../test/import/origin/synthetic.opj"
        ))
        .expect("test/import/origin/synthetic.opj");
        assert_eq!(committed, write_project(&sample()));
    }

    /// The project of the committed sample; regenerate with
    /// `cargo test -p stokesforge-import regenerate_sample -- --ignored`.
    pub(crate) fn sample() -> Project {
        let mut x = Column::numbers("A", OriginDesignation::X, &[400.0, 401.0, 402.0]);
        x.long_name = "Wavelength".into();
        x.unit = "nm".into();
        let mut y = Column::numbers("B", OriginDesignation::Y, &[1.5, 2.5, 3.5]);
        y.long_name = "Signal".into();
        y.unit = "CPS".into();
        y.comment = "first run".into();
        Project {
            books: vec![Book {
                short_name: "Book1".into(),
                long_name: "Sample book".into(),
                sheets: vec![
                    Sheet {
                        name: "Data".into(),
                        columns: vec![x, y],
                    },
                    Sheet {
                        name: "Note".into(),
                        columns: vec![Column {
                            cells: vec![Cell::Text("hello".into()), Cell::Missing],
                            ..Column::numbers("A", OriginDesignation::Disregard, &[])
                        }],
                    },
                ],
                horiba_xml: Some(
                    "<HJYInfo><ExpSummary><ExpType>Spectral Acquisition[Emission]</ExpType>\
                     <SWVersion>3.0.0.0</SWVersion></ExpSummary></HJYInfo>"
                        .into(),
                ),
            }],
            matrices: vec!["Matrix1".into()],
            graphs: vec!["Graph1".into()],
            opju_annotations: true,
            ..Project::default()
        }
    }

    #[test]
    #[ignore = "writes test/import/origin/synthetic.opj and synthetic.opju"]
    fn regenerate_sample() {
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../test/import/origin");
        std::fs::create_dir_all(dir).unwrap();
        std::fs::write(format!("{dir}/synthetic.opj"), write_project(&sample())).unwrap();
        std::fs::write(format!("{dir}/synthetic.opju"), write_opju(&sample())).unwrap();
    }
}

// SPDX-License-Identifier: GPL-3.0-or-later
//
// Port of readDataSetElement / getColumnInfoAndData of liborigin 3.0.3
// (OriginAnyParser.cpp):
//   SPDX-FileCopyrightText: 2017 Miquel Garriga <gbmiquel@gmail.com>
//   SPDX-FileCopyrightText: 2021-2024 Stefan Gerlach <stefan.gerlach@uni.kn>
//   SPDX-License-Identifier: GPL-3.0-or-later
// and of the dataset handling of OriginParser.cpp:
//   SPDX-FileCopyrightText: 2008 Alex Kargovsky <kargovsky@yumr.phys.msu.su>
//   SPDX-FileCopyrightText: 2017 Stefan Gerlach <stefan.gerlach@uni.kn>
//   SPDX-License-Identifier: GPL-2.0-or-later

//! The datasets: one per worksheet column (a name `Book_Column`, with an
//! `@N` suffix for sheet N), matrix or function.

use super::reader::{cstr, le_f64, le_i32, le_u16, Blocks};
use super::OriginError;

/// Origin's "missing value" literal.
const MISSING: f64 = -1.23456789e-300;

pub(crate) struct Dataset {
    pub(crate) kind: DatasetKind,
}

pub(crate) enum DatasetKind {
    Column(ColumnData),
    /// A matrix sheet; the name is that of the matrix (without `@N`).
    Matrix(String),
    Function,
}

pub(crate) struct ColumnData {
    pub(crate) book: String,
    /// 1-based sheet number.
    pub(crate) sheet: usize,
    pub(crate) column: String,
    pub(crate) values: Vec<f64>,
    /// Empty when no cell is text, else one entry per row.
    pub(crate) texts: Vec<Option<String>>,
}

pub(crate) fn read_datasets(blocks: &mut Blocks) -> Result<Vec<Dataset>, OriginError> {
    let mut out = Vec::new();
    while let Some(header) = blocks.object()? {
        let size = blocks.size()?;
        let data = blocks.blob(size)?;
        blocks.object()?; // mask
        out.push(Dataset {
            kind: classify(header, data)?,
        });
    }
    Ok(out)
}

fn classify(header: &[u8], data: &[u8]) -> Result<DatasetKind, OriginError> {
    let short = || OriginError::ShortRecord {
        what: "dataset header",
    };
    let data_type = le_u16(header, 0x16).ok_or_else(short)?;
    le_i32(header, 0x19).ok_or_else(short)?;
    let value_size = match header.get(0x3D).copied().ok_or_else(short)? {
        0 => 8,
        v => usize::from(v),
    };
    let name = cstr(header, 0x58, 25);
    // liborigin splits at the LAST underscore.
    let Some((book, column)) = name.rsplit_once('_').filter(|(_, c)| !c.is_empty()) else {
        return Ok(if data_type == 0x6081 {
            DatasetKind::Function
        } else {
            let base = name.split('@').next().unwrap_or("");
            DatasetKind::Matrix(base.to_string())
        });
    };
    // The sheet number is the `@N` suffix of the column; none means sheet 1.
    let (column, sheet) = match column.rsplit_once('@') {
        Some((c, n)) => (
            c,
            n.trim()
                .parse::<usize>()
                .ok()
                .filter(|&n| n >= 1)
                .unwrap_or(1),
        ),
        None => (column, 1),
    };
    let (values, texts) = decode_cells(data, value_size, data_type);
    Ok(DatasetKind::Column(ColumnData {
        book: book.to_string(),
        sheet,
        column: column.to_string(),
        values,
        texts,
    }))
}

fn number(v: Option<f64>) -> f64 {
    match v {
        Some(v) if v.to_bits() != MISSING.to_bits() => v,
        _ => f64::NAN,
    }
}

/// A text cell; liborigin drops a text holding 0x0E (garbage).
fn text_cell(raw: &[u8]) -> Option<String> {
    if raw.contains(&0x0E) {
        return None;
    }
    let s = cstr(raw, 0, raw.len());
    (!s.is_empty()).then_some(s)
}

fn decode_cells(data: &[u8], value_size: usize, data_type: u16) -> (Vec<f64>, Vec<Option<String>>) {
    let mut values = Vec::with_capacity(data.len() / value_size);
    let mut texts: Vec<Option<String>> = Vec::new();
    for cell in data.chunks_exact(value_size) {
        let (value, text) = if value_size <= 8 {
            // Numeric, date, time...: a plain f64 (narrower sizes are not seen).
            (number(le_f64(cell, 0)), None)
        } else if data_type & 0x100 != 0 {
            // Text & numeric: flag byte, one more byte, then the f64 or the text.
            if cell[0] == 1 {
                (f64::NAN, text_cell(&cell[2..]))
            } else {
                (number(le_f64(cell, 2)), None)
            }
        } else {
            (f64::NAN, text_cell(cell))
        };
        if text.is_some() && texts.len() < values.len() {
            texts.resize(values.len(), None);
        }
        values.push(value);
        if !texts.is_empty() || text.is_some() {
            texts.push(text);
        }
    }
    (values, texts)
}

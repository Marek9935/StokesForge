// SPDX-License-Identifier: GPL-3.0-or-later
//
// Port of readWindowElement / readLayerElement / readAnnotationElement /
// readCurveElement / getCurveProperties of liborigin 3.0.3
// (OriginAnyParser.cpp):
//   SPDX-FileCopyrightText: 2017 Miquel Garriga <gbmiquel@gmail.com>
//   SPDX-FileCopyrightText: 2021-2024 Stefan Gerlach <stefan.gerlach@uni.kn>
//   SPDX-License-Identifier: GPL-3.0-or-later
// and of OriginParser.cpp:
//   SPDX-FileCopyrightText: 2008 Alex Kargovsky <kargovsky@yumr.phys.msu.su>
//   SPDX-FileCopyrightText: 2017 Stefan Gerlach <stefan.gerlach@uni.kn>
//   SPDX-License-Identifier: GPL-2.0-or-later

//! The windows: name, label (long name), layers (sheets) and, in each layer,
//! the curves that describe the columns (book window) or the plotted data
//! (graph window), and the column formulas held by the annotations of a
//! sheet layer. Axis records are walked past, not decoded.

use super::reader::{cstr, le_u16, Blocks};
use super::{OriginDesignation, OriginError};

/// Deepest nesting of annotation groups accepted.
pub(crate) const MAX_ANNOTATION_DEPTH: usize = 64;

pub(crate) struct Window {
    pub(crate) name: String,
    /// The label up to the first `@${`.
    pub(crate) label: String,
    /// What follows the label in the same field (embedded storage blocks).
    pub(crate) storage: String,
    pub(crate) layers: Vec<Layer>,
}

pub(crate) struct Layer {
    pub(crate) sheet_name: String,
    pub(crate) curves: Vec<Curve>,
    /// The annotations of the layer (depth 0) that hold a usable text: the
    /// column short name and the formula. Not yet matched to a column.
    pub(crate) formulas: Vec<Formula>,
}

/// An annotation of a sheet layer: header name (a column short name when it
/// is a formula) and its text.
pub(crate) struct Formula {
    pub(crate) column: String,
    pub(crate) text: String,
}

/// Longest formula text kept, in bytes; a longer one is dropped.
pub(crate) const MAX_FORMULA_BYTES: usize = 64 * 1024;
/// Offset and size of the column short name in an annotation header.
pub(crate) const ANNOTATION_NAME_AT: usize = 0x46;
pub(crate) const ANNOTATION_NAME_LEN: usize = 41;

/// The formula of an annotation from its first block (NUL-terminated text,
/// NUL padded) decoded by `decode`. `None` when the text is blank, longer
/// than [`MAX_FORMULA_BYTES`] or holds a control character other than tab,
/// carriage return and line feed.
pub(crate) fn formula_text(block: &[u8], decode: impl Fn(&[u8]) -> String) -> Option<String> {
    let end = block.iter().position(|&b| b == 0).unwrap_or(block.len());
    let raw = block.get(..end)?;
    if raw.len() > MAX_FORMULA_BYTES {
        return None;
    }
    let text = decode(raw);
    let bad = |c: char| c.is_control() && !matches!(c, '\t' | '\r' | '\n');
    (!text.trim().is_empty() && !text.chars().any(bad)).then_some(text)
}

pub(crate) struct Curve {
    pub(crate) designation: OriginDesignation,
    pub(crate) short_name: String,
    pub(crate) long_name: String,
    pub(crate) unit: String,
    pub(crate) comment: String,
    /// Graph window: dataset rank (1-based, over all datasets in file order)
    /// of the Y and X data, header offsets 0x04 and 0x23. `None` when the
    /// header is too short.
    pub(crate) y_rank: Option<u16>,
    pub(crate) x_rank: Option<u16>,
}

pub(crate) fn read_windows(blocks: &mut Blocks) -> Result<Vec<Window>, OriginError> {
    let mut windows = Vec::new();
    while let Some(header) = blocks.object()? {
        let name = cstr(header, 0x02, 25);
        let full = if header.len() > 0xC3 {
            cstr(header, 0xC3, usize::MAX)
        } else {
            String::new()
        };
        let (label, storage) = match full.find("@${") {
            Some(at) => (full[..at].to_string(), full[at..].to_string()),
            None => (full, String::new()),
        };
        let mut layers = Vec::new();
        while let Some(layer_header) = blocks.object()? {
            layers.push(read_layer(blocks, layer_header)?);
        }
        windows.push(Window {
            name,
            label,
            storage,
            layers,
        });
    }
    Ok(windows)
}

fn read_layer(blocks: &mut Blocks, header: &[u8]) -> Result<Layer, OriginError> {
    let sheet_name = if header.len() > 0xD2 {
        cstr(header, 0xD2, 32)
    } else {
        String::new()
    };
    let mut formulas = Vec::new();
    annotation_list(blocks, 0, &mut formulas)?;
    let mut curves = Vec::new();
    while let Some(curve_header) = blocks.object()? {
        let size = blocks.size()?;
        let data = blocks.blob(size)?;
        curves.push(curve(curve_header, data));
    }
    blocks.skip_list()?; // axis breaks
    for _axis in 0..3 {
        blocks.skip_list()?; // axis parameters x, y, z
    }
    Ok(Layer {
        sheet_name,
        curves,
        formulas,
    })
}

fn curve(header: &[u8], data: &[u8]) -> Curve {
    curve_from(
        header,
        cstr(header, 0x12, usize::MAX),
        &cstr(data, 0, data.len()),
    )
}

/// A curve from its header (designation at 0x11), its short name and its
/// text `LongName\r\nUnit[\r\nComment]`; shared with the `.opju` reader.
pub(crate) fn curve_from(header: &[u8], short_name: String, text: &str) -> Curve {
    let designation = match header.get(0x11) {
        Some(3) => OriginDesignation::X,
        Some(0) => OriginDesignation::Y,
        Some(5) => OriginDesignation::Z,
        Some(6) => OriginDesignation::XError,
        Some(2) => OriginDesignation::YError,
        Some(4) => OriginDesignation::Label,
        _ => OriginDesignation::Disregard,
    };
    let mut parts = text.split("\r\n");
    let long_name = parts.next().unwrap_or("").to_string();
    let unit = parts.next().unwrap_or("").to_string();
    let comment = parts.collect::<Vec<_>>().join("\r\n");
    Curve {
        designation,
        short_name,
        long_name,
        unit,
        comment,
        y_rank: le_u16(header, 0x04),
        x_rank: le_u16(header, 0x23),
    }
}

/// A list of annotations, some of which are groups holding a list. The
/// annotations of depth 0 that hold a usable text are collected in `out`.
fn annotation_list(
    blocks: &mut Blocks,
    depth: usize,
    out: &mut Vec<Formula>,
) -> Result<(), OriginError> {
    if depth > MAX_ANNOTATION_DEPTH {
        return Err(OriginError::TooDeep);
    }
    while let Some(header) = blocks.object()? {
        let first = blocks.size()?;
        let block = blocks.blob(first)?;
        let second = blocks.size()?;
        if (first == 0x5E || first == 0x0A) && second == 4 {
            annotation_list(blocks, depth + 1, &mut Vec::new())?;
        } else {
            blocks.blob(second)?;
            if depth == 0 {
                let name = cstr(header, ANNOTATION_NAME_AT, ANNOTATION_NAME_LEN);
                let text = formula_text(block, |raw| cstr(raw, 0, raw.len()));
                if let (false, Some(text)) = (name.is_empty(), text) {
                    out.push(Formula { column: name, text });
                }
            }
        }
        blocks.object()?;
    }
    Ok(())
}

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
//! the curves that describe the columns. Annotations and axis records are
//! walked past, not decoded.

use super::reader::{cstr, Blocks};
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
}

pub(crate) struct Curve {
    pub(crate) designation: OriginDesignation,
    pub(crate) short_name: String,
    pub(crate) long_name: String,
    pub(crate) unit: String,
    pub(crate) comment: String,
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
    annotation_list(blocks, 0)?;
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
    Ok(Layer { sheet_name, curves })
}

fn curve(header: &[u8], data: &[u8]) -> Curve {
    let designation = match header.get(0x11) {
        Some(3) => OriginDesignation::X,
        Some(0) => OriginDesignation::Y,
        Some(5) => OriginDesignation::Z,
        Some(6) => OriginDesignation::XError,
        Some(2) => OriginDesignation::YError,
        Some(4) => OriginDesignation::Label,
        _ => OriginDesignation::Disregard,
    };
    // `LongName\r\nUnit[\r\nComment]`
    let text = cstr(data, 0, data.len());
    let mut parts = text.split("\r\n");
    let long_name = parts.next().unwrap_or("").to_string();
    let unit = parts.next().unwrap_or("").to_string();
    let comment = parts.collect::<Vec<_>>().join("\r\n");
    Curve {
        designation,
        short_name: cstr(header, 0x12, usize::MAX),
        long_name,
        unit,
        comment,
    }
}

/// A list of annotations, some of which are groups holding a list.
fn annotation_list(blocks: &mut Blocks, depth: usize) -> Result<(), OriginError> {
    if depth > MAX_ANNOTATION_DEPTH {
        return Err(OriginError::TooDeep);
    }
    while blocks.object()?.is_some() {
        let first = blocks.size()?;
        blocks.blob(first)?;
        let second = blocks.size()?;
        if (first == 0x5E || first == 0x0A) && second == 4 {
            annotation_list(blocks, depth + 1)?;
        } else {
            blocks.blob(second)?;
        }
        blocks.object()?;
    }
    Ok(())
}

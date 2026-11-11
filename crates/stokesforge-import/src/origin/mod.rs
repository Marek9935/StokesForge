// SPDX-License-Identifier: GPL-3.0-or-later
//
// Port of a subset of liborigin 3.0.3 (https://sourceforge.net/projects/liborigin/),
// OriginAnyParser.cpp and OriginParser.cpp:
//   SPDX-FileCopyrightText: 2017 Miquel Garriga <gbmiquel@gmail.com>
//   SPDX-FileCopyrightText: 2021-2024 Stefan Gerlach <stefan.gerlach@uni.kn>
//   SPDX-FileCopyrightText: 2008 Alex Kargovsky <kargovsky@yumr.phys.msu.su>
//   SPDX-FileCopyrightText: 2017 Stefan Gerlach <stefan.gerlach@uni.kn>
// OriginAnyParser.cpp is GPL-3.0-or-later, OriginParser.cpp GPL-2.0-or-later;
// the port is GPL-3.0-or-later. liborigin's own history also credits Ion
// Vasilief and Knut Franke. See docs/license-and-extensions.md.

//! Reader of the workbooks of an Origin project file (`.opj`, the binary
//! format Origin 2018 to 2022b still saves and Horiba's software writes).
//!
//! [`read_project`] returns the books (workbooks) with their sheets and
//! columns: names, long names, units, comments, values and text cells.
//! Graphs, matrices and functions are recognised and counted, not read. The
//! `.opju` container of Origin 2023+ is refused with a clear error.
//!
//! Robustness (invariant 10): every read is checked, a defect is an
//! [`OriginError`], never a panic; annotation groups nest at most 64 deep;
//! nothing is allocated from a count the file declares, and every loop
//! consumes bytes, so the work is bounded by the file size.
//!
//! The output uses only this crate's vocabulary (`f64` with `NaN` for a
//! missing value, [`MetadataLine`]); the conversion to model columns belongs
//! to `stokesforge-importer`.

mod dataset;
mod horiba;
mod reader;
#[doc(hidden)]
pub mod synthetic;
mod window;

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use crate::metadata::MetadataLine;
use dataset::{ColumnData, Dataset, DatasetKind};
use reader::{le_f64, Blocks};

/// Why an Origin file could not be read. The `Display` is raw English; the
/// application phrases it from a catalog key.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum OriginError {
    #[error("not an Origin project file")]
    NotOrigin,
    #[error(
        "Origin 2018+ .opju files are not supported: save the project as .opj from Origin 2018 to 2022b, or open it in the free Origin Viewer and save it as .opj"
    )]
    OpjuNotSupported,
    #[error("unsupported Origin file version {version}")]
    UnsupportedVersion { version: f64 },
    #[error("the Origin file is truncated at offset {offset:#x}")]
    Truncated { offset: usize },
    #[error("bad object delimiter at offset {offset:#x}")]
    BadDelimiter { offset: usize },
    #[error("object of {size} bytes at offset {offset:#x} exceeds the file")]
    SizeOutOfRange { offset: usize, size: usize },
    #[error("annotation groups are nested too deeply")]
    TooDeep,
    #[error("{what} is too short")]
    ShortRecord { what: &'static str },
}

/// What a column holds, from Origin's "plot designation".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OriginDesignation {
    X,
    Y,
    Z,
    XError,
    YError,
    Label,
    /// "Disregard" in Origin: no plotting role.
    Disregard,
}

/// A column of a sheet.
#[derive(Debug, Clone, PartialEq)]
pub struct OriginColumn {
    pub short_name: String,
    pub designation: OriginDesignation,
    pub long_name: String,
    pub unit: String,
    pub comment: String,
    /// One value per row; `NaN` for a missing value or a text cell.
    pub values: Vec<f64>,
    /// Empty when no cell is text, else one entry per row (see [`Self::text`]).
    pub texts: Vec<Option<String>>,
}

impl OriginColumn {
    /// The text of row `row`, if that cell is text.
    pub fn text(&self, row: usize) -> Option<&str> {
        self.texts.get(row)?.as_deref()
    }

    pub fn has_text(&self) -> bool {
        self.texts.iter().any(Option::is_some)
    }
}

/// A sheet (Origin "layer") of a book.
#[derive(Debug, Clone, PartialEq)]
pub struct OriginSheet {
    pub name: String,
    pub columns: Vec<OriginColumn>,
}

/// A workbook.
#[derive(Debug, Clone, PartialEq)]
pub struct OriginBook {
    pub short_name: String,
    pub long_name: String,
    pub sheets: Vec<OriginSheet>,
    /// The Horiba information tree flattened, when the book carries one.
    pub info: Vec<MetadataLine>,
}

/// What was recognised and left out.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct OriginSkipped {
    /// Windows with no worksheet data: graphs, notes, layouts.
    pub graphs: usize,
    pub matrices: usize,
    pub functions: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct OriginProject {
    /// The Origin version stored in the file (for example 9.5), when present.
    pub version: Option<f64>,
    pub books: Vec<OriginBook>,
    pub skipped: OriginSkipped,
}

/// Reads the workbooks of an Origin `.opj` file held in memory.
pub fn read_project(bytes: &[u8]) -> Result<OriginProject, OriginError> {
    const MAX_SIGNATURE: usize = 256;
    let line_end = bytes
        .iter()
        .take(MAX_SIGNATURE)
        .position(|&b| b == b'\n')
        .ok_or(OriginError::NotOrigin)?;
    let signature = &bytes[..line_end];
    if signature.starts_with(b"CPYUA") {
        return Err(OriginError::OpjuNotSupported);
    }
    if !signature.starts_with(b"CPYA") {
        return Err(OriginError::NotOrigin);
    }
    let mut blocks = Blocks::new(bytes, line_end + 1);
    let global = blocks.object()?.ok_or(OriginError::ShortRecord {
        what: "global header",
    })?;
    let version = le_f64(global, 0x1B);
    if let Some(v) = version {
        // Versions before 4.0 use another header layout.
        if !(4.0..=1000.0).contains(&v) {
            return Err(OriginError::UnsupportedVersion { version: v });
        }
    }
    if blocks.size()? != 0 {
        return Err(OriginError::BadDelimiter { offset: line_end });
    }
    let datasets = dataset::read_datasets(&mut blocks)?;
    let windows = window::read_windows(&mut blocks)?;
    Ok(assemble(version, datasets, windows))
}

fn assemble(
    version: Option<f64>,
    datasets: Vec<Dataset>,
    windows: Vec<window::Window>,
) -> OriginProject {
    let mut skipped = OriginSkipped::default();
    let mut matrix_names: HashSet<String> = HashSet::new();
    // Column data per book, in file order, taken once by the header that names it.
    let mut by_book: HashMap<String, Vec<Option<ColumnData>>> = HashMap::new();
    for dataset in datasets {
        match dataset.kind {
            DatasetKind::Column(column) => by_book
                .entry(column.book.clone())
                .or_default()
                .push(Some(column)),
            DatasetKind::Matrix(name) => {
                matrix_names.insert(name);
            }
            DatasetKind::Function => skipped.functions += 1,
        }
    }
    let mut books = Vec::new();
    let mut done: HashSet<String> = HashSet::new();
    for window in windows {
        match by_book.remove(&window.name) {
            Some(columns) => {
                done.insert(window.name.clone());
                books.push(build_book(window, columns));
            }
            None if done.contains(&window.name) => {}
            None if matrix_names.contains(&window.name) => skipped.matrices += 1,
            None => skipped.graphs += 1,
        }
    }
    OriginProject {
        version,
        books,
        skipped,
    }
}

fn build_book(window: window::Window, mut data: Vec<Option<ColumnData>>) -> OriginBook {
    let mut index: HashMap<(usize, &str), usize> = HashMap::new();
    for (i, column) in data.iter().enumerate() {
        if let Some(c) = column {
            index.entry((c.sheet, c.column.as_str())).or_insert(i);
        }
    }
    // Which dataset each header curve takes (looked up before any is moved).
    let mut taken: Vec<Vec<Option<usize>>> = Vec::new();
    let mut claimed: HashSet<usize> = HashSet::new();
    for (n, layer) in window.layers.iter().enumerate() {
        let sheet = n + 1;
        taken.push(
            layer
                .curves
                .iter()
                .map(|curve| {
                    let i = *index.get(&(sheet, curve.short_name.as_str()))?;
                    claimed.insert(i).then_some(i)
                })
                .collect(),
        );
    }
    // Sheets: every layer, plus any sheet number only the data mentions.
    let mut sheet_numbers: BTreeSet<usize> = (1..=window.layers.len()).collect();
    sheet_numbers.extend(data.iter().flatten().map(|c| c.sheet));
    let mut columns: BTreeMap<usize, Vec<OriginColumn>> = BTreeMap::new();
    for (n, layer) in window.layers.iter().enumerate() {
        for (curve, source) in layer.curves.iter().zip(&taken[n]) {
            if curve.short_name.is_empty() {
                continue;
            }
            let (values, texts) = match source.and_then(|i| data[i].take()) {
                Some(c) => (c.values, c.texts),
                None => (Vec::new(), Vec::new()),
            };
            columns.entry(n + 1).or_default().push(OriginColumn {
                short_name: curve.short_name.clone(),
                designation: curve.designation,
                long_name: curve.long_name.clone(),
                unit: curve.unit.clone(),
                comment: curve.comment.clone(),
                values,
                texts,
            });
        }
    }
    // Datasets no header names keep their place after the described columns.
    for column in data.into_iter().flatten() {
        columns.entry(column.sheet).or_default().push(OriginColumn {
            short_name: column.column,
            designation: OriginDesignation::Disregard,
            long_name: String::new(),
            unit: String::new(),
            comment: String::new(),
            values: column.values,
            texts: column.texts,
        });
    }
    let sheets = sheet_numbers
        .into_iter()
        .map(|number| OriginSheet {
            name: window
                .layers
                .get(number - 1)
                .map(|l| l.sheet_name.clone())
                .unwrap_or_default(),
            columns: columns.remove(&number).unwrap_or_default(),
        })
        .collect();
    OriginBook {
        short_name: window.name,
        long_name: window.label,
        sheets,
        info: horiba::info_lines(&window.storage),
    }
}

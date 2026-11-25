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

//! Reader of the workbooks of an Origin project file: `.opj` (the binary
//! format Origin 2018 to 2022b still saves and Horiba's software writes) and
//! `.opju` (the `CPYUA` container of Origin 2018 and later).
//!
//! [`read_project`] returns the books (workbooks) with their sheets and
//! columns: names, long names, units, comments, formulas, values and text
//! cells, and the graphs with their layers and curves resolved to columns.
//! Matrices and functions are recognised and counted, not read. Both
//! containers give the same [`OriginProject`]; the `.opju` one is walked by
//! `opju` (grammar in its module docs). An `.opju` whose window section is
//! not understood is read from its datasets and window objects as far as they
//! go, and [`OriginProject::partial`] says so.
//!
//! Column formulas: in a book window, each sheet layer lists annotations.
//! The annotation header holds at 0x46 (41 bytes) a name, and its first
//! block a NUL-terminated text. When the name is the short name of a column
//! of THAT sheet and the text is not blank, the text is the column's command
//! ([`OriginColumn::formula`]); the other annotations (`SCT`, `SPC_*`,
//! `__*`, `Text`...) are not formulas. Only annotations at depth 0 count. A
//! text longer than 64 KiB or holding a control character other than tab, CR
//! and LF is refused (`None`, never an error). The formula is Origin's own
//! text (`col(B)*2`, `raw_data!A`): it is not translated here.
//!
//! Graphs: a window with at least one layer that no dataset names (so it is
//! not a book, nor a matrix) is a graph; one with no layer is a note or a
//! layout and is only counted in [`OriginSkipped::graphs`]. Each curve record
//! of a graph layer holds at header offsets 0x04 (Y) and 0x23 (X) the 1-based
//! rank of its dataset among ALL the datasets of the file in file order
//! (matrices and functions count). The ranks are resolved to
//! [`OriginColumnAt`] (book, sheet, column positions in
//! [`OriginProject::books`]) once every book is built, since a graph window
//! may precede its book; a rank of 0, past the end or naming a dataset
//! that is not in a book gives `None`.
//!
//! Robustness (invariant 10): every read is checked, a defect is an
//! [`OriginError`], never a panic; annotation groups nest at most 64 deep
//! (`.opj`; the `.opju` reader does not recurse); nothing is allocated from a
//! count the file declares, and every loop consumes bytes, so the work is
//! bounded by the file size (an `.opju` also has a cell budget).
//!
//! The output uses only this crate's vocabulary (`f64` with `NaN` for a
//! missing value, [`MetadataLine`]); the conversion to model columns belongs
//! to `stokesforge-importer`.

mod dataset;
mod horiba;
mod opju;
mod opju_cells;
mod opju_codec;
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
    #[error("unsupported Origin .opju structure at offset {offset:#x}: {what}")]
    OpjuUnsupported { offset: usize, what: &'static str },
    #[error("the Origin file declares more than {limit} cells")]
    TooManyCells { limit: usize },
    #[error("the Origin file declares more than {limit} bytes of text cells")]
    TooMuchText { limit: usize },
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
    /// The column's formula as Origin wrote it (the "command" of the column),
    /// when it has one; see the module docs.
    pub formula: Option<String>,
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

/// Where a plotted dataset lives: positions in [`OriginProject::books`],
/// then [`OriginBook::sheets`], then [`OriginSheet::columns`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OriginColumnAt {
    pub book: usize,
    pub sheet: usize,
    pub column: usize,
}

/// A plotted curve of a graph layer; `None` when the dataset cannot be
/// resolved to a column of a book.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OriginCurve {
    pub y: Option<OriginColumnAt>,
    pub x: Option<OriginColumnAt>,
}

/// A layer of a graph window.
#[derive(Debug, Clone, PartialEq)]
pub struct OriginGraphLayer {
    pub name: String,
    pub curves: Vec<OriginCurve>,
}

/// A graph window.
#[derive(Debug, Clone, PartialEq)]
pub struct OriginGraph {
    pub short_name: String,
    pub long_name: String,
    pub layers: Vec<OriginGraphLayer>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct OriginProject {
    /// The Origin version stored in the file (for example 9.5), when present.
    pub version: Option<f64>,
    pub books: Vec<OriginBook>,
    /// The graph windows in file order, curves resolved to columns of `books`.
    pub graphs: Vec<OriginGraph>,
    pub skipped: OriginSkipped,
    /// `true` when the window section of an `.opju` was not understood to the
    /// end: the books of the windows read are complete, the others come from
    /// their datasets alone (dataset short names, no long name, designation
    /// `Disregard`, sheets numbered from the `@N` suffix with empty names) and
    /// the counts in `skipped` are incomplete. Always `false` for an `.opj`.
    pub partial: bool,
}

/// Reads the workbooks of an Origin `.opj` or `.opju` file held in memory.
pub fn read_project(bytes: &[u8]) -> Result<OriginProject, OriginError> {
    const MAX_SIGNATURE: usize = 256;
    let line_end = bytes
        .iter()
        .take(MAX_SIGNATURE)
        .position(|&b| b == b'\n')
        .ok_or(OriginError::NotOrigin)?;
    let signature = &bytes[..line_end];
    if signature.starts_with(b"CPYUA") {
        return opju::read(bytes, line_end + 1);
    }
    if !signature.starts_with(b"CPYA") {
        return Err(OriginError::NotOrigin);
    }
    let mut blocks = Blocks::new(bytes, line_end + 1);
    let global = blocks.object()?.ok_or(OriginError::ShortRecord {
        what: "global header",
    })?;
    let version = check_version(global)?;
    if blocks.size()? != 0 {
        return Err(OriginError::BadDelimiter { offset: line_end });
    }
    let datasets = dataset::read_datasets(&mut blocks)?;
    let windows = window::read_windows(&mut blocks)?;
    Ok(assemble(version, datasets, windows, false))
}

/// The version of the global header, refused when it is too old.
fn check_version(global: &[u8]) -> Result<Option<f64>, OriginError> {
    let version = le_f64(global, 0x1B);
    if let Some(v) = version {
        // Versions before 4.0 use another header layout.
        if !(4.0..=1000.0).contains(&v) {
            return Err(OriginError::UnsupportedVersion { version: v });
        }
    }
    Ok(version)
}

fn assemble(
    version: Option<f64>,
    datasets: Vec<Dataset>,
    windows: Vec<window::Window>,
    partial: bool,
) -> OriginProject {
    let mut skipped = OriginSkipped::default();
    let mut matrix_names: HashSet<String> = HashSet::new();
    // Column data per book, in file order, taken once by the header that names it.
    let mut by_book: HashMap<String, Vec<Option<ColumnData>>> = HashMap::new();
    let mut book_order: Vec<String> = Vec::new();
    // The place of every dataset by rank (file order, matrices and functions
    // included): book, sheet number and column name of a worksheet column.
    let mut places: Vec<Option<(String, usize, String)>> = Vec::new();
    for dataset in datasets {
        places.push(match &dataset.kind {
            DatasetKind::Column(c) => Some((c.book.clone(), c.sheet, c.column.clone())),
            _ => None,
        });
        match dataset.kind {
            DatasetKind::Column(column) => {
                if !by_book.contains_key(&column.book) {
                    book_order.push(column.book.clone());
                }
                by_book
                    .entry(column.book.clone())
                    .or_default()
                    .push(Some(column));
            }
            DatasetKind::Matrix(name) => {
                matrix_names.insert(name);
            }
            DatasetKind::Function => skipped.functions += 1,
        }
    }
    let mut books = Vec::new();
    // Sheet numbers of each book, parallel to `books`.
    let mut numbers: Vec<Vec<usize>> = Vec::new();
    let mut done: HashSet<String> = HashSet::new();
    let mut graph_windows = Vec::new();
    for window in windows {
        match by_book.remove(&window.name) {
            Some(columns) => {
                done.insert(window.name.clone());
                let (book, sheet_numbers) = build_book(window, columns);
                books.push(book);
                numbers.push(sheet_numbers);
            }
            None if done.contains(&window.name) => {}
            None if matrix_names.contains(&window.name) => skipped.matrices += 1,
            None => {
                skipped.graphs += 1;
                if !window.layers.is_empty() {
                    graph_windows.push(window);
                }
            }
        }
    }
    if partial {
        // The window section was cut short: the books it did not describe
        // are built from their datasets alone, in file order.
        for name in book_order {
            if let Some(columns) = by_book.remove(&name) {
                let window = window::Window {
                    name,
                    label: String::new(),
                    storage: String::new(),
                    layers: Vec::new(),
                };
                let (mut book, sheet_numbers) = build_book(window, columns);
                // No header named the sheets: Origin's default names.
                for (index, sheet) in book.sheets.iter_mut().enumerate() {
                    if sheet.name.is_empty() {
                        sheet.name = format!("Sheet{}", index + 1);
                    }
                }
                books.push(book);
                numbers.push(sheet_numbers);
            }
        }
    }
    let locator = Locator::new(&books, &numbers, &places);
    let graphs = graph_windows
        .into_iter()
        .map(|window| locator.graph(window))
        .collect();
    OriginProject {
        version,
        books,
        graphs,
        skipped,
        partial,
    }
}

/// Resolves dataset ranks to positions in the books, once all are built.
struct Locator<'a> {
    books: &'a [OriginBook],
    numbers: &'a [Vec<usize>],
    places: &'a [Option<(String, usize, String)>],
    by_name: HashMap<&'a str, usize>,
}

impl<'a> Locator<'a> {
    fn new(
        books: &'a [OriginBook],
        numbers: &'a [Vec<usize>],
        places: &'a [Option<(String, usize, String)>],
    ) -> Self {
        let mut by_name = HashMap::new();
        for (i, book) in books.iter().enumerate() {
            by_name.entry(book.short_name.as_str()).or_insert(i);
        }
        Self {
            books,
            numbers,
            places,
            by_name,
        }
    }

    /// The column of the dataset of 1-based `rank`, if it is in a book.
    fn at(&self, rank: Option<u16>) -> Option<OriginColumnAt> {
        let (book_name, number, column_name) = self
            .places
            .get(usize::from(rank?).checked_sub(1)?)?
            .as_ref()?;
        let book = *self.by_name.get(book_name.as_str())?;
        let sheet = self.numbers.get(book)?.iter().position(|n| n == number)?;
        let column = self
            .books
            .get(book)?
            .sheets
            .get(sheet)?
            .columns
            .iter()
            .position(|c| &c.short_name == column_name)?;
        Some(OriginColumnAt {
            book,
            sheet,
            column,
        })
    }

    fn graph(&self, window: window::Window) -> OriginGraph {
        OriginGraph {
            short_name: window.name,
            long_name: window.label,
            layers: window
                .layers
                .into_iter()
                .map(|layer| OriginGraphLayer {
                    name: layer.sheet_name,
                    curves: layer
                        .curves
                        .iter()
                        .map(|c| OriginCurve {
                            y: self.at(c.y_rank),
                            x: self.at(c.x_rank),
                        })
                        .collect(),
                })
                .collect(),
        }
    }
}

/// The book and the sheet numbers of its sheets, in order.
fn build_book(
    window: window::Window,
    mut data: Vec<Option<ColumnData>>,
) -> (OriginBook, Vec<usize>) {
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
                formula: None,
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
            formula: None,
        });
    }
    // Formulas: by column short name, in the sheet of the layer only.
    for (n, layer) in window.layers.iter().enumerate() {
        let Some(sheet_columns) = columns.get_mut(&(n + 1)) else {
            continue;
        };
        for formula in &layer.formulas {
            if let Some(column) = sheet_columns
                .iter_mut()
                .find(|c| c.short_name == formula.column)
            {
                column.formula.get_or_insert_with(|| formula.text.clone());
            }
        }
    }
    let numbers: Vec<usize> = sheet_numbers.iter().copied().collect();
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
    let book = OriginBook {
        short_name: window.name,
        long_name: window.label,
        sheets,
        info: horiba::info_lines(&window.storage),
    };
    (book, numbers)
}

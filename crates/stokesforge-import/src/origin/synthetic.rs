// SPDX-License-Identifier: GPL-3.0-or-later

//! Test generator: writes a minimal valid `.opj` from a description, so the
//! reader is tested without a real (private, copyrighted) Origin file. The
//! framing and offsets are those the reader takes from liborigin 3.0.3. Not
//! part of the product API.

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

#[derive(Debug, Clone)]
pub struct Column {
    pub short_name: String,
    pub designation: OriginDesignation,
    pub long_name: String,
    pub unit: String,
    pub comment: String,
    pub cells: Vec<Cell>,
    pub storage: Cells,
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
}

impl Default for Project {
    fn default() -> Self {
        Self {
            version: 9.5,
            books: Vec::new(),
            matrices: Vec::new(),
            graphs: Vec::new(),
            annotation_nesting: 0,
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
    fn committed_sample_is_the_generator_output() {
        let committed = std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../test/import/origin/synthetic.opj"
        ))
        .expect("test/import/origin/synthetic.opj");
        assert_eq!(committed, write_project(&sample()));
    }

    /// The project of the committed sample; regenerate with
    /// `STOKESFORGE_REGEN_OPJ=1 cargo test -p stokesforge-import regenerate_sample`.
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
            ..Project::default()
        }
    }

    #[test]
    #[ignore = "writes test/import/origin/synthetic.opj"]
    fn regenerate_sample() {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../test/import/origin/synthetic.opj"
        );
        std::fs::create_dir_all(std::path::Path::new(path).parent().unwrap()).unwrap();
        std::fs::write(path, write_project(&sample())).unwrap();
    }
}

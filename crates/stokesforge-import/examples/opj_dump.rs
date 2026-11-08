// SPDX-License-Identifier: GPL-3.0-or-later
// Throwaway prototype for #490: lists the workbooks of an Origin .opj file.
// Port of a subset of liborigin 3.0.3 (https://sourceforge.net/projects/liborigin/),
// OriginAnyParser.cpp and OriginParser.cpp:
//   Copyright 2005-2008, 2017-2024 Stefan Gerlach
//   Copyright 2017 Miquel Garriga
//   Copyright 2007-2009 Alex Kargovsky
//   Copyright 2007-2010 Ion Vasilief
//   Copyright 2010 Knut Franke
//
// Usage:
//   cargo run -p stokesforge-import --example opj_dump -- <file.opj>
//   cargo run -p stokesforge-import --example opj_dump -- <file.opj> --dump <Book> <Sheet> <Column>
//
// Feasibility prototype only: no product code, every read is bounds-checked.

use std::process::ExitCode;

type Res<T> = Result<T, String>;

/// Origin's "missing value" literal.
const MISSING: f64 = -1.23456789e-300;

#[derive(Clone, Debug)]
enum Cell {
    Num(f64),
    Text(String),
}

impl Cell {
    fn is_missing(&self) -> bool {
        match self {
            Cell::Num(v) => v.is_nan(),
            Cell::Text(s) => s.is_empty(),
        }
    }
}

struct Dataset {
    book: String,
    sheet: usize,
    col: String,
    data_type: u16,
    value_size: usize,
    total_rows: i32,
    cells: Vec<Cell>,
}

impl Dataset {
    fn kind(&self) -> &'static str {
        if self.value_size <= 8 {
            "numeric"
        } else if self.data_type & 0x100 != 0 {
            "text&numeric"
        } else {
            "text"
        }
    }
}

struct Curve {
    designation: u8,
    short: String,
    comment: String,
}

struct Layer {
    sheet_name: String,
    curves: Vec<Curve>,
}

struct Window {
    name: String,
    label: String,
    layers: Vec<Layer>,
}

struct Reader<'a> {
    b: &'a [u8],
    p: usize,
}

impl<'a> Reader<'a> {
    fn size(&mut self) -> Res<usize> {
        let at = self.p;
        let raw = self
            .b
            .get(at..at + 5)
            .ok_or_else(|| format!("unexpected end of file reading a size at offset {at:#x}"))?;
        if raw[4] != 0x0A {
            return Err(format!("bad size delimiter at offset {at:#x}"));
        }
        let v = u32::from_le_bytes([raw[0], raw[1], raw[2], raw[3]]);
        self.p = at + 5;
        Ok(v as usize)
    }

    fn blob(&mut self, n: usize) -> Res<&'a [u8]> {
        if n == 0 {
            return Ok(&[]);
        }
        let at = self.p;
        let end = at
            .checked_add(n)
            .ok_or_else(|| format!("object size overflow at offset {at:#x}"))?;
        let data = self
            .b
            .get(at..end)
            .ok_or_else(|| format!("truncated object of {n} bytes at offset {at:#x}"))?;
        if self.b.get(end) != Some(&0x0A) {
            return Err(format!("bad object delimiter at offset {end:#x}"));
        }
        self.p = end + 1;
        Ok(data)
    }

    /// Reads a size then the object it announces; `None` for the size-0 end mark.
    fn object(&mut self) -> Res<Option<&'a [u8]>> {
        let n = self.size()?;
        if n == 0 {
            Ok(None)
        } else {
            self.blob(n).map(Some)
        }
    }

    fn skip_list(&mut self) -> Res<()> {
        while self.object()?.is_some() {}
        Ok(())
    }
}

fn latin1(b: &[u8]) -> String {
    b.iter().map(|&c| c as char).collect()
}

/// NUL-terminated string at `off`, at most `max` bytes.
fn cstr(b: &[u8], off: usize, max: usize) -> String {
    let Some(rest) = b.get(off..) else {
        return String::new();
    };
    let rest = &rest[..rest.len().min(max)];
    let end = rest.iter().position(|&c| c == 0).unwrap_or(rest.len());
    latin1(&rest[..end])
}

fn le_u16(b: &[u8], off: usize) -> Res<u16> {
    let s = b.get(off..off + 2).ok_or("header too short")?;
    Ok(u16::from_le_bytes([s[0], s[1]]))
}

fn le_i32(b: &[u8], off: usize) -> Res<i32> {
    let s = b.get(off..off + 4).ok_or("header too short")?;
    Ok(i32::from_le_bytes([s[0], s[1], s[2], s[3]]))
}

fn le_f64(b: &[u8], off: usize) -> Option<f64> {
    let s = b.get(off..off + 8)?;
    let mut a = [0u8; 8];
    a.copy_from_slice(s);
    Some(f64::from_le_bytes(a))
}

fn num(v: f64) -> Cell {
    if v.to_bits() == MISSING.to_bits() {
        Cell::Num(f64::NAN)
    } else {
        Cell::Num(v)
    }
}

fn decode_cells(data: &[u8], vs: usize, data_type: u16) -> Vec<Cell> {
    let n = data.len() / vs;
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let cell = &data[i * vs..(i + 1) * vs];
        if vs <= 8 {
            // Narrower value sizes are not seen in practice; read the f64 when it fits.
            out.push(le_f64(cell, 0).map_or(Cell::Num(f64::NAN), num));
        } else if data_type & 0x100 != 0 {
            if cell[0] == 1 {
                out.push(Cell::Text(cstr(cell, 2, vs - 2)));
            } else {
                out.push(le_f64(cell, 2).map_or(Cell::Num(f64::NAN), num));
            }
        } else {
            out.push(Cell::Text(cstr(cell, 0, vs)));
        }
    }
    out
}

fn annotation_list(r: &mut Reader, depth: usize) -> Res<()> {
    if depth > 64 {
        return Err("annotation nesting too deep".into());
    }
    while r.object()?.is_some() {
        let s1 = r.size()?;
        r.blob(s1)?;
        let s2 = r.size()?;
        if (s1 == 0x5E || s1 == 0x0A) && s2 == 4 {
            annotation_list(r, depth + 1)?;
        } else {
            r.blob(s2)?;
        }
        r.object()?;
    }
    Ok(())
}

struct Parsed {
    signature: String,
    version: Option<f64>,
    datasets: Vec<Dataset>,
    windows: Vec<Window>,
    skipped_non_column: usize,
    end: usize,
}

fn parse(b: &[u8]) -> Res<Parsed> {
    let nl = b
        .iter()
        .position(|&c| c == b'\n')
        .ok_or("no signature line: not an Origin project file")?;
    let signature = latin1(&b[..nl]);
    if !signature.starts_with("CPYA") {
        return Err("not an Origin project file (missing CPYA signature)".into());
    }
    let mut r = Reader { b, p: nl + 1 };
    let gh = r.object()?.ok_or("empty global header")?;
    let version = le_f64(gh, 0x1B);
    if r.size()? != 0 {
        return Err("global header is not followed by an end mark".into());
    }

    let mut datasets = Vec::new();
    let mut skipped_non_column = 0;
    while let Some(h) = r.object()? {
        let ds = r.size()?;
        let data = r.blob(ds)?;
        r.object()?;
        let name = cstr(h, 0x58, 25);
        let data_type = le_u16(h, 0x16)?;
        let vs = match h.get(0x3D).copied().ok_or("header too short")? {
            0 => 8,
            v => v as usize,
        };
        let total_rows = le_i32(h, 0x19)?;
        let Some((book, rest)) = name.split_once('_') else {
            skipped_non_column += 1;
            continue;
        };
        let (col, sheet) = match rest.split_once('@') {
            Some((c, s)) => (c, s.parse::<usize>().unwrap_or(1)),
            None => (rest, 1),
        };
        datasets.push(Dataset {
            book: book.to_string(),
            sheet,
            col: col.to_string(),
            data_type,
            value_size: vs,
            total_rows,
            cells: decode_cells(data, vs, data_type),
        });
    }

    let mut windows = Vec::new();
    while let Some(wh) = r.object()? {
        let name = cstr(wh, 0x02, 25);
        let label = if wh.len() > 0xC3 {
            let s = cstr(wh, 0xC3, usize::MAX);
            s.split("@${").next().unwrap_or("").to_string()
        } else {
            String::new()
        };
        let mut layers = Vec::new();
        while let Some(lh) = r.object()? {
            let sheet_name = if lh.len() > 0xD2 {
                cstr(lh, 0xD2, 32)
            } else {
                String::new()
            };
            annotation_list(&mut r, 0)?;
            let mut curves = Vec::new();
            while let Some(ch) = r.object()? {
                let cds = r.size()?;
                let cd = r.blob(cds)?;
                let designation = match ch.get(0x11).copied().unwrap_or(255) {
                    3 => b'X',
                    0 => b'Y',
                    5 => b'Z',
                    6 => b'x', // XErr
                    2 => b'y', // YErr
                    4 => b'L',
                    _ => b'-',
                };
                curves.push(Curve {
                    designation,
                    short: cstr(ch, 0x12, usize::MAX),
                    comment: cstr(cd, 0, usize::MAX),
                });
            }
            r.skip_list()?; // axis breaks
            for _ in 0..3 {
                r.skip_list()?; // axis parameters x, y, z
            }
            layers.push(Layer { sheet_name, curves });
        }
        windows.push(Window {
            name,
            label,
            layers,
        });
    }
    Ok(Parsed {
        signature,
        version,
        datasets,
        windows,
        skipped_non_column,
        end: r.p,
    })
}

fn designation_name(d: u8) -> &'static str {
    match d {
        b'X' => "X",
        b'Y' => "Y",
        b'Z' => "Z",
        b'x' => "XErr",
        b'y' => "YErr",
        b'L' => "Label",
        _ => "None",
    }
}

fn find<'a>(p: &'a Parsed, book: &str, sheet: usize, col: &str) -> Option<&'a Dataset> {
    p.datasets
        .iter()
        .find(|d| d.book == book && d.sheet == sheet && d.col == col)
}

fn print_tree(p: &Parsed, file_len: usize) {
    println!("signature: {}", p.signature);
    match p.version {
        Some(v) => println!("version: {v}"),
        None => println!("version: unknown"),
    }
    let mut books = 0;
    let mut sheets = 0;
    let mut columns = 0;
    let mut graphs = 0;
    for w in &p.windows {
        if !p.datasets.iter().any(|d| d.book == w.name) {
            graphs += 1;
            continue;
        }
        books += 1;
        let max_ds_sheet = p
            .datasets
            .iter()
            .filter(|d| d.book == w.name)
            .map(|d| d.sheet)
            .max()
            .unwrap_or(0);
        let nsheets = w.layers.len().max(max_ds_sheet);
        println!("BOOK {} long={:?} sheets={}", w.name, w.label, nsheets);
        sheets += nsheets;
        for s in 1..=nsheets {
            let layer = w.layers.get(s - 1);
            println!(
                "  SHEET {} name={:?}",
                s,
                layer.map_or("?", |l| l.sheet_name.as_str())
            );
            let mut seen: Vec<&str> = Vec::new();
            if let Some(l) = layer {
                for c in &l.curves {
                    if c.short.is_empty() {
                        continue;
                    }
                    seen.push(&c.short);
                    let mut parts = c.comment.split("\r\n");
                    let long = parts.next().unwrap_or("");
                    let units = parts.next().unwrap_or("");
                    let comment = parts.collect::<Vec<_>>().join("\r\n");
                    columns += 1;
                    match find(p, &w.name, s, &c.short) {
                        Some(d) => println!(
                            "    COL {} {} long={:?} units={:?} comment={:?} rows={} kind={} missing={}",
                            c.short,
                            designation_name(c.designation),
                            long,
                            units,
                            comment,
                            d.cells.len(),
                            d.kind(),
                            d.cells.iter().filter(|x| x.is_missing()).count()
                        ),
                        None => println!(
                            "    COL {} {} long={:?} units={:?} comment={:?} (no data)",
                            c.short,
                            designation_name(c.designation),
                            long,
                            units,
                            comment
                        ),
                    }
                }
            }
            for d in p
                .datasets
                .iter()
                .filter(|d| d.book == w.name && d.sheet == s && !seen.contains(&d.col.as_str()))
            {
                columns += 1;
                println!(
                    "    COL {} (no header) rows={} (hdr total {}) kind={} missing={}",
                    d.col,
                    d.cells.len(),
                    d.total_rows,
                    d.kind(),
                    d.cells.iter().filter(|x| x.is_missing()).count()
                );
            }
        }
    }
    println!(
        "totals: books={books} sheets={sheets} columns={columns} datasets={} windows={} graphs_skipped={graphs} matrices_or_functions_skipped={}",
        p.datasets.len(),
        p.windows.len(),
        p.skipped_non_column
    );
    println!("walk stopped at offset {:#x} of {:#x}", p.end, file_len);
}

fn dump(p: &Parsed, book: &str, sheet: usize, col: &str) -> Res<()> {
    use std::io::Write;
    let d = find(p, book, sheet, col).ok_or("no such column")?;
    let mut text = String::new();
    for c in &d.cells {
        match c {
            Cell::Num(v) => text.push_str(&format!("{v}\n")),
            Cell::Text(s) => text.push_str(&format!("{s:?}\n")),
        }
    }
    // A closed pipe (e.g. `| head`) is not an error worth a panic.
    let _ = std::io::stdout().lock().write_all(text.as_bytes());
    Ok(())
}

fn run(args: &[String]) -> Res<()> {
    let path = args
        .first()
        .ok_or("usage: opj_dump <file.opj> [--dump <Book> <Sheet> <Column>]")?;
    let bytes = std::fs::read(path).map_err(|e| format!("cannot read file: {e}"))?;
    let parsed = parse(&bytes)?;
    if args.get(1).map(String::as_str) == Some("--dump") {
        let (Some(book), Some(sheet), Some(col)) = (args.get(2), args.get(3), args.get(4)) else {
            return Err("--dump needs <Book> <Sheet> <Column>".into());
        };
        let sheet: usize = sheet.parse().map_err(|_| "sheet index must be a number")?;
        dump(&parsed, book, sheet, col)
    } else {
        print_tree(&parsed, bytes.len());
        Ok(())
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

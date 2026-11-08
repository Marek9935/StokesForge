// SPDX-License-Identifier: GPL-3.0-or-later

//! The Origin `.opju` reader, on files written by the synthetic generator
//! (`origin::synthetic::write_opju`), compared with the `.opj` the same
//! description gives.

use stokesforge_import::origin::synthetic::{
    fpc_encode, lz4_literal, opju_header_code, put_leb, put_zigzag, write_opju,
    write_opju_with_dataset, write_project, Book, Cell, Cells, Column, OpjuNumbers, Project, Sheet,
    OPJU_TRAILER_LEN, STRING_TABLE_KEY,
};
use stokesforge_import::origin::{read_project, OriginDesignation, OriginError, OriginProject};

fn numbers(name: &str, designation: OriginDesignation, values: &[f64]) -> Column {
    Column::numbers(name, designation, values)
}

fn sheet(name: &str, columns: Vec<Column>) -> Sheet {
    Sheet {
        name: name.into(),
        columns,
    }
}

fn book(short: &str, sheets: Vec<Sheet>) -> Book {
    Book {
        short_name: short.into(),
        long_name: format!("{short} long"),
        sheets,
        horiba_xml: None,
    }
}

fn one_column(column: Column) -> Project {
    Project {
        books: vec![book("Book1", vec![sheet("Data", vec![column])])],
        ..Project::default()
    }
}

/// Every field of a project as text, values as bit patterns (a `NaN` is one
/// pattern), so two projects compare with `assert_eq!`.
fn flat(project: &OriginProject) -> Vec<String> {
    let mut out = vec![format!(
        "{:?} {:?} partial={}",
        project.version, project.skipped, project.partial
    )];
    for b in &project.books {
        out.push(format!(
            "book {} / {} / {:?}",
            b.short_name, b.long_name, b.info
        ));
        for s in &b.sheets {
            out.push(format!("sheet {}", s.name));
            for c in &s.columns {
                let bits: Vec<u64> = c
                    .values
                    .iter()
                    .map(|v| if v.is_nan() { u64::MAX } else { v.to_bits() })
                    .collect();
                out.push(format!(
                    "col {} {:?} {} {} {} {bits:x?} {:?}",
                    c.short_name, c.designation, c.long_name, c.unit, c.comment, c.texts
                ));
            }
        }
    }
    out
}

fn linear() -> Vec<f64> {
    (0..300).map(|i| 1000.0 + f64::from(i) * 0.5).collect()
}

/// Values that take every path of the cell encoding.
fn awkward_values() -> Vec<f64> {
    let mut v = vec![
        0.0,
        -0.0,
        5e-324, // subnormal: a single low bit
        -5e-324,
        1.0,
        1.0,
        1.0,
        1.0, // a repeat
        f64::MAX,
        f64::MIN_POSITIVE,
        f64::INFINITY,
        f64::NEG_INFINITY,
        -1.5,
        0.1,
        f64::NAN,
        2.0,
        2.0, // a pair, not a repeat
        4096.0,
    ];
    v.extend(linear());
    for _ in 0..40 {
        v.extend([1.5, 2.5]); // an FCM pattern
    }
    v
}

fn rich_project() -> Project {
    let mut x = numbers("A", OriginDesignation::X, &awkward_values());
    x.long_name = "Wavelength".into();
    x.unit = "nm".into();
    x.comment = "line one\r\nline two".into();
    let mut y = numbers("B", OriginDesignation::Y, &awkward_values());
    y.cells.insert(3, Cell::Missing);
    y.cells.push(Cell::Missing);
    y.long_name = "Signal".into();
    y.unit = "CPS".into();
    y.opju_numbers = OpjuNumbers::Runs;
    let mut single = numbers("C", OriginDesignation::YError, &awkward_values());
    single.opju_numbers = OpjuNumbers::Single;
    single.unit = "\u{B5}m".into();
    let mut mixed = numbers("D", OriginDesignation::Label, &[]);
    mixed.cells = vec![
        Cell::Number(2.0),
        Cell::Text("a longer text".into()),
        Cell::Text("a longer text".into()),
        Cell::Text("a longer text".into()),
        Cell::Text("other".into()),
        Cell::Text(String::new()),
        Cell::Missing,
        Cell::Text("a longer text".into()),
        Cell::Number(7.0),
    ];
    let mut by_reference = mixed.clone();
    by_reference.short_name = "E".into();
    by_reference.opju_text_by_reference = true;
    let mut singles = by_reference.clone();
    singles.short_name = "F".into();
    singles.opju_numbers = OpjuNumbers::Single;
    let mut text_only = numbers("G", OriginDesignation::Label, &[]);
    text_only.storage = Cells::Text;
    text_only.cells = vec![
        Cell::Text("one".into()),
        Cell::Text("two words".into()),
        Cell::Number(3.0),
    ];
    let mut numeric_storage = numbers("H", OriginDesignation::Z, &[1.0]);
    numeric_storage.storage = Cells::Numeric;
    numeric_storage.cells.push(Cell::Text("lost".into()));
    let mut empty = numbers("I", OriginDesignation::Disregard, &[]);
    empty.long_name = "No rows".into();
    let mut horiba = book(
        "Book2",
        vec![sheet(
            "Data",
            vec![numbers("A", OriginDesignation::Y, &[1.0, 2.0])],
        )],
    );
    horiba.horiba_xml = Some(
        "<ExpProperties><SlitWidth>1</SlitWidth></ExpProperties><HJYInfo><ExpSummary>\
         <ExpType>Spectral Acquisition[Emission]</ExpType><IntegrationTime>0.5</IntegrationTime>\
         </ExpSummary></HJYInfo>"
            .into(),
    );
    Project {
        books: vec![
            book(
                "Book1",
                vec![
                    sheet("Data", vec![x, y, single, mixed]),
                    sheet("Strings", vec![by_reference, singles, text_only]),
                    sheet("Misc", vec![numeric_storage, empty]),
                    sheet("Nothing", vec![]),
                ],
            ),
            horiba,
            book(
                "My_Book",
                vec![sheet("S", vec![numbers("Z", OriginDesignation::Y, &[9.0])])],
            ),
        ],
        matrices: vec!["Matrix1".into(), "Matrix2".into()],
        graphs: vec!["Graph1".into(), "Graph2".into(), "Graph3".into()],
        opju_annotations: true,
        ..Project::default()
    }
}

#[test]
fn the_opj_and_the_opju_of_the_same_project_read_equal() {
    let project = rich_project();
    let from_opj = read_project(&write_project(&project)).unwrap();
    let from_opju = read_project(&write_opju(&project)).unwrap();
    assert_eq!(flat(&from_opju), flat(&from_opj));
    assert!(!from_opju.partial);
    assert_eq!(from_opju.books.len(), 3);
    assert_eq!(from_opju.skipped.graphs, 3);
    assert_eq!(from_opju.skipped.matrices, 2);
}

#[test]
fn utf8_text_and_windows_1252_text_both_read() {
    let mut col = numbers("A", OriginDesignation::Y, &[1.0]);
    col.unit = "\u{B5}m".into();
    col.long_name = "Caf\u{E9}".into();
    col.cells = vec![Cell::Text("5 \u{20AC}".into())];
    for utf8 in [false, true] {
        let project = Project {
            opju_utf8: utf8,
            ..one_column(col.clone())
        };
        let bytes = write_opju(&project);
        // The two encodings differ on the wire.
        assert_eq!(
            bytes.windows(2).any(|w| w == [0xC2, 0xB5])
                || bytes.windows(2).any(|w| w == [0xC3, 0xA9]),
            utf8
        );
        let read = read_project(&bytes).unwrap();
        let c = &read.books[0].sheets[0].columns[0];
        assert_eq!(
            (c.unit.as_str(), c.long_name.as_str()),
            ("\u{B5}m", "Caf\u{E9}")
        );
        assert_eq!(c.text(0), Some("5 \u{20AC}"));
    }
}

fn expected_bits(cell: &Cell) -> Option<u64> {
    match cell {
        Cell::Number(v) if v.is_nan() => None,
        Cell::Number(v) => Some(v.to_bits()),
        Cell::Missing | Cell::Text(_) => None,
    }
}

#[test]
fn values_are_bit_identical_on_every_encoding_path() {
    for mode in [OpjuNumbers::Auto, OpjuNumbers::Runs, OpjuNumbers::Single] {
        let mut col = numbers("A", OriginDesignation::Y, &awkward_values());
        col.cells.insert(5, Cell::Missing);
        col.cells
            .extend([Cell::Missing, Cell::Missing, Cell::Missing]);
        col.opju_numbers = mode;
        let read = read_project(&write_opju(&one_column(col.clone()))).unwrap();
        let values = &read.books[0].sheets[0].columns[0].values;
        assert_eq!(values.len(), col.cells.len(), "{mode:?}");
        for (i, (got, cell)) in values.iter().zip(&col.cells).enumerate() {
            match expected_bits(cell) {
                Some(bits) => assert_eq!(got.to_bits(), bits, "{mode:?} cell {i}"),
                None => assert!(got.is_nan(), "{mode:?} cell {i}"),
            }
        }
    }
}

#[test]
fn a_negative_zero_and_a_subnormal_keep_their_bits() {
    for mode in [OpjuNumbers::Auto, OpjuNumbers::Runs, OpjuNumbers::Single] {
        let mut col = numbers("A", OriginDesignation::Y, &[-0.0, 0.0, 5e-324]);
        col.opju_numbers = mode;
        let read = read_project(&write_opju(&one_column(col))).unwrap();
        let v = &read.books[0].sheets[0].columns[0].values;
        assert_eq!(v[0].to_bits(), (-0.0f64).to_bits(), "{mode:?}");
        assert_eq!(v[1].to_bits(), 0, "{mode:?}");
        assert_eq!(v[2].to_bits(), 1, "{mode:?}");
    }
}

#[test]
fn text_cells_inline_and_by_reference_read_the_same() {
    let cells = vec![
        Cell::Text("alpha".into()),
        Cell::Text("alpha".into()),
        Cell::Text("beta".into()),
        Cell::Number(1.5),
        Cell::Text(String::new()),
        Cell::Text("caf\u{E9}".into()),
        Cell::Text("alpha".into()),
    ];
    for by_reference in [false, true] {
        for mode in [OpjuNumbers::Auto, OpjuNumbers::Single] {
            let mut col = numbers("A", OriginDesignation::Label, &[]);
            col.cells = cells.clone();
            col.opju_text_by_reference = by_reference;
            col.opju_numbers = mode;
            let read = read_project(&write_opju(&one_column(col))).unwrap();
            let c = &read.books[0].sheets[0].columns[0];
            let got: Vec<_> = (0..cells.len()).map(|r| c.text(r)).collect();
            assert_eq!(
                got,
                [
                    Some("alpha"),
                    Some("alpha"),
                    Some("beta"),
                    None,
                    None,
                    Some("caf\u{E9}"),
                    Some("alpha")
                ],
                "by reference {by_reference}, {mode:?}"
            );
            assert_eq!(c.values[3], 1.5);
            assert!(c.values[0].is_nan() && c.values[4].is_nan());
        }
    }
}

#[test]
fn descriptions_designations_and_sheet_names_are_read() {
    let read = read_project(&write_opju(&rich_project())).unwrap();
    let b = &read.books[0];
    assert_eq!(
        (b.short_name.as_str(), b.long_name.as_str()),
        ("Book1", "Book1 long")
    );
    let names: Vec<_> = b.sheets.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(names, ["Data", "Strings", "Misc", "Nothing"]);
    let c = &b.sheets[0].columns;
    assert_eq!(c[0].designation, OriginDesignation::X);
    assert_eq!(
        (c[0].long_name.as_str(), c[0].unit.as_str()),
        ("Wavelength", "nm")
    );
    assert_eq!(c[0].comment, "line one\r\nline two");
    assert_eq!(c[1].designation, OriginDesignation::Y);
    assert_eq!(c[2].designation, OriginDesignation::YError);
    assert_eq!(c[3].designation, OriginDesignation::Label);
    assert_eq!(b.sheets[1].columns[0].short_name, "E");
    assert_eq!(b.sheets[2].columns[0].designation, OriginDesignation::Z);
    assert_eq!(b.sheets[2].columns[1].long_name, "No rows");
    assert!(b.sheets[3].columns.is_empty());
    assert_eq!(read.books[2].short_name, "My_Book");
}

#[test]
fn the_horiba_tree_gives_the_lines_the_opj_gives() {
    let project = rich_project();
    let from_opj = read_project(&write_project(&project)).unwrap();
    let from_opju = read_project(&write_opju(&project)).unwrap();
    assert!(!from_opju.books[1].info.is_empty());
    assert_eq!(from_opju.books[1].info, from_opj.books[1].info);
    assert_eq!(from_opju.books[1].info[0].key, "ExpProperties.SlitWidth");
    assert!(from_opju.books[0].info.is_empty());
}

#[test]
fn a_preview_before_the_global_header_is_skipped() {
    let project = Project {
        opju_preview_len: 37,
        ..rich_project()
    };
    let read = read_project(&write_opju(&project)).unwrap();
    assert_eq!(
        flat(&read),
        flat(&read_project(&write_opju(&rich_project())).unwrap())
    );
}

#[test]
fn a_header_with_an_old_version_is_refused() {
    let project = Project {
        version: 3.5,
        ..rich_project()
    };
    assert!(matches!(
        read_project(&write_opju(&project)),
        Err(OriginError::UnsupportedVersion { .. })
    ));
}

// Partial reads.

fn two_books_the_second_broken() -> Project {
    let mut first = numbers("A", OriginDesignation::X, &[1.0, 2.0]);
    first.long_name = "Long".into();
    first.unit = "nm".into();
    Project {
        books: vec![
            book("Book1", vec![sheet("Data", vec![first])]),
            book(
                "Book2",
                vec![
                    sheet("First", vec![numbers("A", OriginDesignation::X, &[3.0])]),
                    sheet(
                        "Second",
                        vec![numbers("B", OriginDesignation::Y, &[4.0, 5.0])],
                    ),
                ],
            ),
        ],
        graphs: vec!["Graph1".into()],
        opju_bad_annotation: Some(1),
        ..Project::default()
    }
}

#[test]
fn a_broken_annotation_in_the_second_book_keeps_the_first_whole() {
    let read = read_project(&write_opju(&two_books_the_second_broken())).unwrap();
    assert!(read.partial);
    assert_eq!(read.books.len(), 2);
    let first = &read.books[0];
    assert_eq!(
        (first.short_name.as_str(), first.long_name.as_str()),
        ("Book1", "Book1 long")
    );
    assert_eq!(first.sheets[0].name, "Data");
    let a = &first.sheets[0].columns[0];
    assert_eq!(
        (
            a.designation,
            a.long_name.as_str(),
            a.unit.as_str(),
            a.values.as_slice()
        ),
        (OriginDesignation::X, "Long", "nm", &[1.0, 2.0][..])
    );
    // The second book comes from its datasets alone.
    let second = &read.books[1];
    assert_eq!(
        (second.short_name.as_str(), second.long_name.as_str()),
        ("Book2", "")
    );
    assert!(second.info.is_empty());
    assert_eq!(second.sheets.len(), 2);
    assert!(second.sheets.iter().all(|s| s.name.is_empty()));
    let b = &second.sheets[1].columns[0];
    assert_eq!(
        (
            b.short_name.as_str(),
            b.designation,
            b.long_name.as_str(),
            b.values.as_slice()
        ),
        ("B", OriginDesignation::Disregard, "", &[4.0, 5.0][..])
    );
    assert_eq!(second.sheets[0].columns[0].values, [3.0]);
}

#[test]
fn a_complete_opju_and_an_opj_are_not_partial() {
    let project = rich_project();
    assert!(!read_project(&write_opju(&project)).unwrap().partial);
    assert!(!read_project(&write_project(&project)).unwrap().partial);
}

#[test]
fn datasets_of_several_books_become_books_in_file_order_when_no_window_survives() {
    let project = Project {
        opju_bad_annotation: Some(0),
        ..Project {
            books: vec![
                book(
                    "Zed",
                    vec![sheet("S", vec![numbers("A", OriginDesignation::Y, &[1.0])])],
                ),
                book(
                    "Alpha",
                    vec![sheet("S", vec![numbers("A", OriginDesignation::Y, &[2.0])])],
                ),
            ],
            ..Project::default()
        }
    };
    let read = read_project(&write_opju(&project)).unwrap();
    assert!(read.partial);
    let names: Vec<_> = read.books.iter().map(|b| b.short_name.as_str()).collect();
    assert_eq!(names, ["Zed", "Alpha"]);
}

// Defects.

fn cell_block_offset(bytes: &[u8], cells: &[u8]) -> usize {
    let mut needle = cells.to_vec();
    needle.extend_from_slice(&[0xCE, 0xFA]);
    bytes
        .windows(needle.len())
        .position(|w| w == needle)
        .expect("the cell block is in the file")
}

fn dataset_with_cells(cells: &[u8]) -> Vec<u8> {
    write_opju_with_dataset(&opju_header_code("Book1_A"), cells, &[])
}

#[test]
fn a_broken_dataset_is_an_error_with_its_offset() {
    let cells = [0x07, 0x00];
    let bytes = dataset_with_cells(&cells);
    let at = cell_block_offset(&bytes, &cells);
    let err = read_project(&bytes).unwrap_err();
    assert_eq!(
        err,
        OriginError::OpjuUnsupported {
            offset: at,
            what: "unknown cell block flag"
        }
    );
    let text = err.to_string();
    assert!(text.contains(&format!("{at:#x}")) && text.contains("unknown cell block flag"));
}

/// A cell block of `rows` rows from the items given.
fn block(flag: u8, rows: u64, items: &[u8]) -> Vec<u8> {
    let mut cells = vec![flag];
    put_leb(&mut cells, rows);
    cells.extend_from_slice(items);
    cells
}

fn run(m: u64, kind: u8) -> Vec<u8> {
    let mut v = vec![0xFF, 0xFF];
    put_leb(&mut v, m);
    v.push(kind);
    v
}

fn assert_unsupported(cells: &[u8], what: &str) {
    match read_project(&dataset_with_cells(cells)) {
        Err(OriginError::OpjuUnsupported { what: got, .. }) => {
            assert!(got.contains(what), "{got} for {what}");
        }
        other => panic!("{what}: {other:?}"),
    }
}

#[test]
fn malformed_cell_blocks_are_errors() {
    let mut item = run(2, 0);
    put_zigzag(&mut item, 3);
    item.push(0x64);
    assert_unsupported(&block(5, 2, &item), "overruns its run");
    let mut item = run(2, 0);
    put_zigzag(&mut item, 0);
    assert_unsupported(&block(5, 2, &item), "empty segment");
    let mut item = run(1, 0);
    put_zigzag(&mut item, 1);
    item.push(0x66);
    assert_unsupported(&block(5, 1, &item), "unknown value tag");
    let mut item = run(1, 0);
    put_zigzag(&mut item, -1);
    item.extend_from_slice(&[0x0D, 0x01, 0x05]);
    assert_unsupported(&block(5, 1, &item), "FPC segment marker");
    assert_unsupported(&block(5, 1, &run(1, 2)), "unknown run kind");
    assert_unsupported(&block(5, 1, &[0x02, 0x00]), "unknown cell item");
    assert_unsupported(&block(5, 1, &run(5, 0)), "run overruns");
}

#[test]
fn a_truncated_cell_block_is_a_truncation() {
    let cells = block(5, 2, &[0x00, 0x00, 0x64]);
    assert!(matches!(
        read_project(&dataset_with_cells(&cells)),
        Err(OriginError::Truncated { .. } | OriginError::OpjuUnsupported { .. })
    ));
}

fn string_table_props(table: &[u8], dsz: u32) -> Vec<u8> {
    let mut payload = dsz.to_le_bytes().to_vec();
    payload.extend_from_slice(&lz4_literal(table));
    let mut props = STRING_TABLE_KEY.to_vec();
    props.extend_from_slice(&[0x05, 0x01]);
    put_leb(&mut props, payload.len() as u64);
    props.push(0);
    props.extend_from_slice(&payload);
    props
}

fn table_bytes(strings: &[&str]) -> Vec<u8> {
    let mut t = vec![0x77, 0x11, 0x11, 0x11];
    t.extend_from_slice(&[0u8; 20]);
    for s in strings {
        t.extend_from_slice(s.as_bytes());
        t.push(0);
    }
    t
}

fn reference_cell(reference: u64) -> Vec<u8> {
    let mut items = vec![0x01, 0x00];
    put_leb(&mut items, reference);
    block(4, 1, &items)
}

#[test]
fn string_references_are_checked() {
    let table = table_bytes(&["ab", "cd"]);
    let props = string_table_props(&table, table.len() as u32);
    let read = |reference, props: &[u8]| {
        read_project(&write_opju_with_dataset(
            &opju_header_code("Book1_A"),
            &reference_cell(reference),
            props,
        ))
    };
    // Reference k is byte k-1 of the strings, which start after 24 bytes.
    let col = |p: &OriginProject| p.books[0].sheets[0].columns[0].text(0).map(str::to_string);
    assert!(read(1, &props).is_ok(), "a valid reference reads");
    // Probe the texts through a project with a window.
    let mut c = numbers("A", OriginDesignation::Label, &[]);
    c.cells = vec![
        Cell::Text("ab".into()),
        Cell::Text("cd".into()),
        Cell::Text("ab".into()),
    ];
    c.opju_text_by_reference = true;
    let p = read_project(&write_opju(&one_column(c))).unwrap();
    assert_eq!(col(&p), Some("ab".to_string()));
    assert_eq!(p.books[0].sheets[0].columns[0].text(1), Some("cd"));
    // Outside the table, and no table at all.
    assert!(matches!(
        read(99, &props),
        Err(OriginError::OpjuUnsupported { what, .. }) if what.contains("outside")
    ));
    assert!(matches!(
        read(1, &[1, 2, 3]),
        Err(OriginError::OpjuUnsupported { what, .. }) if what.contains("without a string table")
    ));
    // A table with the wrong magic, or a size that lies.
    let mut bad = table.clone();
    bad[0] = 0;
    assert!(read(1, &string_table_props(&bad, bad.len() as u32)).is_err());
    assert!(read(1, &string_table_props(&table, table.len() as u32 + 1)).is_err());
    assert!(read(1, &string_table_props(&table, table.len() as u32 - 1)).is_err());
}

// Robustness (invariant 10).

#[test]
fn every_prefix_of_a_valid_opju_is_a_result_not_a_panic() {
    let project = rich_project();
    let bytes = write_opju(&project);
    let windows_end = bytes.len() - OPJU_TRAILER_LEN;
    let full = read_project(&bytes).unwrap();
    assert!(!full.partial);
    let mut first_ok = None;
    for len in 0..=bytes.len() {
        let result = read_project(&bytes[..len]);
        if len >= windows_end {
            assert!(!result.unwrap().partial, "prefix of {len} bytes");
            continue;
        }
        match result {
            Err(_) => assert!(first_ok.is_none(), "an error after a success at {len}"),
            Ok(read) => {
                first_ok.get_or_insert(len);
                // Everything the datasets hold is there, the windows are not.
                assert!(read.partial, "prefix of {len} bytes");
                assert!(!read.books.is_empty());
            }
        }
    }
    assert!(
        first_ok.is_some(),
        "the window section is cut in the middle"
    );
}

#[test]
fn flipping_any_bit_never_panics() {
    let project = Project {
        books: vec![book(
            "Book1",
            vec![sheet(
                "Data",
                vec![
                    numbers("A", OriginDesignation::X, &[1.0, 1.0, 1.0, 2.5, 3.5, 3.5]),
                    {
                        let mut t = numbers("B", OriginDesignation::Label, &[]);
                        t.cells = vec![
                            Cell::Text("x".into()),
                            Cell::Text("x".into()),
                            Cell::Number(1.0),
                        ];
                        t.opju_text_by_reference = true;
                        t
                    },
                ],
            )],
        )],
        graphs: vec!["Graph1".into()],
        opju_annotations: true,
        ..Project::default()
    };
    let bytes = write_opju(&project);
    let mut results = 0usize;
    for at in 0..bytes.len() {
        for bit in 0..8 {
            let mut mutated = bytes.clone();
            mutated[at] ^= 1 << bit;
            if read_project(&mutated).is_ok() {
                results += 1;
            }
        }
    }
    // Many flips fall in text or values and still read.
    assert!(results > 0);
}

#[test]
fn a_huge_repeat_count_is_refused_by_the_cell_budget() {
    let mut item = run(1 << 40, 0);
    put_zigzag(&mut item, 1 << 40);
    item.push(0x64);
    let started = std::time::Instant::now();
    let err = read_project(&dataset_with_cells(&block(5, 1 << 40, &item))).unwrap_err();
    assert!(matches!(err, OriginError::TooManyCells { .. }), "{err:?}");
    assert!(started.elapsed().as_secs() < 2);
}

#[test]
fn a_cell_budget_allows_what_the_file_size_justifies() {
    // A million equal cells in a few bytes: within 1_000_000 + 8 * file size.
    let mut item = run(1_000_000, 0);
    put_zigzag(&mut item, 1_000_000);
    item.push(0x64);
    let bytes = dataset_with_cells(&block(5, 1_000_000, &item));
    // No window: the dataset is read, then dropped with its book.
    assert!(read_project(&bytes).is_ok());
    let mut item = run(1_100_000, 0);
    put_zigzag(&mut item, 1_100_000);
    item.push(0x64);
    let err = read_project(&dataset_with_cells(&block(5, 1_100_000, &item))).unwrap_err();
    assert!(matches!(err, OriginError::TooManyCells { limit } if limit > 1_000_000));
}

#[test]
fn a_huge_row_count_with_few_cells_is_an_error() {
    let mut item = run(1, 0);
    put_zigzag(&mut item, 1);
    item.push(0x64);
    let err = read_project(&dataset_with_cells(&block(5, 1 << 50, &item))).unwrap_err();
    // The row count claims far more cells than the file holds.
    assert!(
        matches!(
            err,
            OriginError::OpjuUnsupported { .. } | OriginError::Truncated { .. }
        ),
        "{err:?}"
    );
}

#[test]
fn a_huge_declared_lz4_size_is_an_error() {
    let table = table_bytes(&["x"]);
    for dsz in [u32::MAX, 1 << 24, (1 << 24) + 1, 1 << 20] {
        let props = string_table_props(&table, dsz);
        let result = read_project(&write_opju_with_dataset(
            &opju_header_code("Book1_A"),
            &reference_cell(1),
            &props,
        ));
        assert!(
            matches!(result, Err(OriginError::OpjuUnsupported { .. })),
            "{dsz}"
        );
    }
}

#[test]
fn a_zero_run_bomb_is_refused() {
    let bomb = vec![0xBFu8; 20_000]; // 66 bytes each: 1.3 MB
    let result = read_project(&write_opju_with_dataset(&bomb, &block(5, 0, &[]), &[]));
    assert!(matches!(
        result,
        Err(OriginError::OpjuUnsupported { what, .. }) if what.contains("too large")
    ));
}

#[test]
fn an_fpc_count_beyond_the_data_is_refused_before_decoding() {
    let mut item = run(500_000, 0);
    put_zigzag(&mut item, -500_000);
    item.extend_from_slice(&[0x0C, 0x11, 0x01, 0x02]);
    let err = read_project(&dataset_with_cells(&block(5, 500_000, &item))).unwrap_err();
    assert!(
        matches!(err, OriginError::OpjuUnsupported { what, .. } if what.contains("FPC value count")),
        "{err:?}"
    );
}

#[test]
fn a_text_cell_repeated_beyond_the_text_budget_is_refused() {
    let text = vec![b'a'; 100_000];
    let mut item = run(2000, 1);
    put_zigzag(&mut item, 2000);
    put_leb(&mut item, text.len() as u64);
    item.extend_from_slice(&text);
    let err = read_project(&dataset_with_cells(&block(5, 2000, &item))).unwrap_err();
    assert!(matches!(err, OriginError::TooMuchText { .. }), "{err:?}");
}

#[test]
fn many_tiny_fpc_segments_cost_time_proportional_to_their_values() {
    // 500 000 segments of one value each: the 64 KiB of FPC tables must not
    // be cleared once per segment.
    let n = 500_000u64;
    let mut item = run(n, 0);
    for _ in 0..n {
        put_zigzag(&mut item, -1);
        item.extend_from_slice(&[0x0C, 0x01, 0x05]); // one residual byte, value 5
    }
    let bytes = dataset_with_cells(&block(5, n, &item));
    let started = std::time::Instant::now();
    assert!(read_project(&bytes).is_ok());
    let elapsed = started.elapsed();
    assert!(elapsed.as_secs_f64() < 3.0, "took {elapsed:?}");
}

#[test]
fn an_fpc_segment_is_decoded_by_the_reader_the_same_as_the_writer_encodes_it() {
    // The encoder is independent of the decoder: a linear ramp takes the
    // DFCM path, a repeating pair the FCM path, and both are shorter than
    // 8 bytes a value.
    let ramp: Vec<u64> = (0..100u64)
        .map(|i| 0x4000_0000_0000_0000 + i * 0x10_0000)
        .collect();
    let bytes = fpc_encode(&ramp);
    assert!(bytes.len() < ramp.len() * 3, "{}", bytes.len());
    let pair: Vec<u64> = (0..100)
        .map(|i| {
            if i % 2 == 0 {
                0x3FF8 << 48
            } else {
                0x4004 << 48
            }
        })
        .collect();
    assert!(fpc_encode(&pair).len() < pair.len() * 3);
}

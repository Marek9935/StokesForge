// SPDX-License-Identifier: GPL-3.0-or-later

//! The Origin `.opj` reader, on files written by the synthetic generator.

use stokesforge_import::origin::synthetic::{
    write_project, Book, Cell, Cells, Column, Project, Sheet, FIRST_DATASET_OFFSET,
};
use stokesforge_import::origin::{read_project, OriginDesignation, OriginError};

fn column(name: &str, designation: OriginDesignation, values: &[f64]) -> Column {
    Column::numbers(name, designation, values)
}

fn book(short: &str, sheets: Vec<Sheet>) -> Book {
    Book {
        short_name: short.into(),
        long_name: format!("{short} long"),
        sheets,
        horiba_xml: None,
    }
}

fn sheet(name: &str, columns: Vec<Column>) -> Sheet {
    Sheet {
        name: name.into(),
        columns,
    }
}

fn one_book(columns: Vec<Column>) -> Project {
    Project {
        books: vec![book("Book1", vec![sheet("Data", columns)])],
        ..Project::default()
    }
}

#[test]
fn books_sheets_columns_and_their_descriptions() {
    let mut x = column("A", OriginDesignation::X, &[400.0, 401.0]);
    x.long_name = "Wavelength".into();
    x.unit = "nm".into();
    let mut y = column("B", OriginDesignation::Y, &[1.5, 2.5]);
    y.long_name = "Signal".into();
    y.unit = "CPS".into();
    y.comment = "line one\r\nline two".into();
    let project = Project {
        books: vec![book(
            "Book1",
            vec![sheet("Data", vec![x, y]), sheet("Note", vec![])],
        )],
        ..Project::default()
    };
    let read = read_project(&write_project(&project)).unwrap();
    assert_eq!(read.version, Some(9.5));
    assert_eq!(read.books.len(), 1);
    let b = &read.books[0];
    assert_eq!(
        (b.short_name.as_str(), b.long_name.as_str()),
        ("Book1", "Book1 long")
    );
    let names: Vec<_> = b.sheets.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(names, ["Data", "Note"]);
    let [cx, cy] = &b.sheets[0].columns[..] else {
        panic!("two columns expected");
    };
    assert_eq!(cx.designation, OriginDesignation::X);
    assert_eq!(
        (cx.long_name.as_str(), cx.unit.as_str()),
        ("Wavelength", "nm")
    );
    assert_eq!(cx.values, [400.0, 401.0]);
    assert_eq!(cy.designation, OriginDesignation::Y);
    assert_eq!((cy.long_name.as_str(), cy.unit.as_str()), ("Signal", "CPS"));
    assert_eq!(cy.comment, "line one\r\nline two");
    assert!(b.sheets[1].columns.is_empty());
}

#[test]
fn a_missing_value_is_nan() {
    let mut col = column("A", OriginDesignation::Y, &[1.0]);
    col.cells = vec![Cell::Number(1.0), Cell::Missing, Cell::Number(3.0)];
    for storage in [Cells::Mixed, Cells::Numeric] {
        col.storage = storage;
        let read = read_project(&write_project(&one_book(vec![col.clone()]))).unwrap();
        let values = &read.books[0].sheets[0].columns[0].values;
        assert_eq!(values.len(), 3);
        assert_eq!(values[0], 1.0);
        assert!(values[1].is_nan(), "{storage:?}");
        assert_eq!(values[2], 3.0);
    }
}

#[test]
fn a_text_cell_reads_as_text_and_its_value_is_nan() {
    let mut col = column("A", OriginDesignation::Label, &[]);
    col.cells = vec![
        Cell::Number(2.0),
        Cell::Text("a longer text".into()),
        Cell::Missing,
    ];
    let read = read_project(&write_project(&one_book(vec![col]))).unwrap();
    let c = &read.books[0].sheets[0].columns[0];
    assert_eq!(c.text(0), None);
    assert_eq!(c.text(1), Some("a longer text"));
    assert_eq!(c.text(2), None);
    assert!(c.values[1].is_nan());
    assert_eq!(c.values[0], 2.0);
}

#[test]
fn a_text_only_column_reads_as_text() {
    let mut col = column("A", OriginDesignation::Label, &[]);
    col.storage = Cells::Text;
    col.cells = vec![Cell::Text("one".into()), Cell::Text("two words".into())];
    let read = read_project(&write_project(&one_book(vec![col]))).unwrap();
    let c = &read.books[0].sheets[0].columns[0];
    assert_eq!((c.text(0), c.text(1)), (Some("one"), Some("two words")));
}

#[test]
fn a_numeric_column_holds_no_text() {
    let read = read_project(&write_project(&one_book(vec![column(
        "A",
        OriginDesignation::Y,
        &[1.0, 2.0],
    )])))
    .unwrap();
    assert!(!read.books[0].sheets[0].columns[0].has_text());
}

#[test]
fn the_name_is_split_at_the_last_underscore() {
    let project = Project {
        books: vec![book(
            "My_Book",
            vec![sheet(
                "Data",
                vec![column("A", OriginDesignation::Y, &[1.0])],
            )],
        )],
        ..Project::default()
    };
    let read = read_project(&write_project(&project)).unwrap();
    assert_eq!(read.books.len(), 1);
    assert_eq!(read.books[0].short_name, "My_Book");
    assert_eq!(read.books[0].sheets[0].columns[0].short_name, "A");
}

#[test]
fn an_at_suffix_selects_the_sheet() {
    let project = Project {
        books: vec![book(
            "Book1",
            vec![
                sheet("First", vec![column("A", OriginDesignation::Y, &[1.0])]),
                sheet("Second", vec![column("A", OriginDesignation::Y, &[2.0])]),
            ],
        )],
        ..Project::default()
    };
    let read = read_project(&write_project(&project)).unwrap();
    let sheets = &read.books[0].sheets;
    assert_eq!(sheets.len(), 2);
    assert_eq!(sheets[0].columns[0].values, [1.0]);
    assert_eq!(sheets[1].name, "Second");
    assert_eq!(sheets[1].columns[0].values, [2.0]);
}

#[test]
fn graphs_matrices_are_skipped_and_counted() {
    let project = Project {
        matrices: vec!["Matrix1".into(), "Matrix2".into()],
        graphs: vec!["Graph1".into(), "Graph2".into(), "Graph3".into()],
        ..one_book(vec![column("A", OriginDesignation::Y, &[1.0])])
    };
    let read = read_project(&write_project(&project)).unwrap();
    assert_eq!(read.books.len(), 1);
    assert_eq!(read.skipped.graphs, 3);
    assert_eq!(read.skipped.matrices, 2);
    assert_eq!(read.skipped.functions, 0);
}

#[test]
fn the_horiba_tree_becomes_metadata_lines() {
    let mut b = book(
        "Book1",
        vec![sheet(
            "Data",
            vec![column("A", OriginDesignation::Y, &[1.0])],
        )],
    );
    b.horiba_xml = Some(
        "<ExpProperties><SlitWidth>1</SlitWidth></ExpProperties><HJYInfo><ExpSummary>\
         <ExpType>Spectral Acquisition[Emission]</ExpType><IntegrationTime>0.5</IntegrationTime>\
         <SWVersion>3.0.0.0</SWVersion></ExpSummary></HJYInfo>"
            .into(),
    );
    let project = Project {
        books: vec![b],
        ..Project::default()
    };
    let read = read_project(&write_project(&project)).unwrap();
    let info: Vec<_> = read.books[0]
        .info
        .iter()
        .map(|l| (l.key.as_str(), l.value.as_str()))
        .collect();
    assert_eq!(
        info,
        [
            ("ExpProperties.SlitWidth", "1"),
            (
                "HJYInfo.ExpSummary.ExpType",
                "Spectral Acquisition[Emission]"
            ),
            ("HJYInfo.ExpSummary.IntegrationTime", "0.5"),
            ("HJYInfo.ExpSummary.SWVersion", "3.0.0.0"),
        ]
    );
    assert_eq!(read.books[0].long_name, "Book1 long");
}

#[test]
fn text_is_windows_1252() {
    let mut col = column("A", OriginDesignation::Y, &[1.0]);
    col.unit = "\u{B5}m".into(); // byte 0xB5
    col.cells = vec![Cell::Text("5 \u{20AC}".into())];
    let bytes = write_project(&one_book(vec![col]));
    assert!(bytes.contains(&0xB5) && !bytes.windows(2).any(|w| w == [0xC2, 0xB5]));
    let read = read_project(&bytes).unwrap();
    let c = &read.books[0].sheets[0].columns[0];
    assert_eq!(c.unit, "\u{B5}m");
    assert_eq!(c.text(0), Some("5 \u{20AC}"));
}

#[test]
fn the_synthetic_file_ends_with_its_last_mark() {
    let bytes = write_project(&one_book(vec![column("A", OriginDesignation::Y, &[1.0])]));
    assert!(bytes.ends_with(&[0, 0, 0, 0, 0x0A]));
}

// Robustness (invariant 10).

fn sample_bytes() -> Vec<u8> {
    let mut x = column("A", OriginDesignation::X, &[1.0, 2.0]);
    x.cells.push(Cell::Text("t".into()));
    let mut b = book("Book1", vec![sheet("Data", vec![x])]);
    b.horiba_xml = Some("<HJYInfo><A>1</A></HJYInfo>".into());
    write_project(&Project {
        books: vec![b],
        matrices: vec!["Matrix1".into()],
        graphs: vec!["Graph1".into()],
        ..Project::default()
    })
}

#[test]
fn every_prefix_of_a_valid_file_is_an_error_not_a_panic() {
    let bytes = sample_bytes();
    assert!(read_project(&bytes).is_ok());
    for len in 0..bytes.len() {
        assert!(
            read_project(&bytes[..len]).is_err(),
            "prefix of {len} bytes"
        );
    }
}

#[test]
fn a_huge_declared_size_is_an_error_without_a_big_allocation() {
    let mut bytes = sample_bytes();
    bytes[FIRST_DATASET_OFFSET..FIRST_DATASET_OFFSET + 4].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(matches!(
        read_project(&bytes),
        Err(OriginError::SizeOutOfRange { .. })
    ));
}

#[test]
fn a_huge_row_count_allocates_nothing_from_it() {
    // The row count field claims 2^31 - 1 rows; the data holds a few.
    let mut bytes = sample_bytes();
    let rows = FIRST_DATASET_OFFSET + 5 + 0x19;
    bytes[rows..rows + 4].copy_from_slice(&i32::MAX.to_le_bytes());
    let read = read_project(&bytes).unwrap();
    assert_eq!(read.books[0].sheets[0].columns[0].values.len(), 3);
}

#[test]
fn annotation_groups_nest_at_most_64_deep() {
    let nested = |depth| {
        write_project(&Project {
            annotation_nesting: depth,
            ..one_book(vec![column("A", OriginDesignation::Y, &[1.0])])
        })
    };
    assert!(read_project(&nested(64)).is_ok());
    assert_eq!(read_project(&nested(65)), Err(OriginError::TooDeep));
    // Far deeper than the stack could follow.
    assert_eq!(read_project(&nested(5000)), Err(OriginError::TooDeep));
}

#[test]
fn an_opju_file_is_refused_with_advice() {
    let err = read_project(b"CPYUA 4.3268 195\n\0\0\0\0\0").unwrap_err();
    assert_eq!(err, OriginError::OpjuNotSupported);
    let text = err.to_string();
    assert!(
        text.contains(".opj") && text.contains("Origin Viewer"),
        "{text}"
    );
}

#[test]
fn other_files_are_not_origin_files() {
    assert_eq!(read_project(b""), Err(OriginError::NotOrigin));
    assert_eq!(read_project(b"\n"), Err(OriginError::NotOrigin));
    assert_eq!(read_project(b"CPY"), Err(OriginError::NotOrigin));
    assert_eq!(
        read_project(b"wavelength;signal\n400;1\n"),
        Err(OriginError::NotOrigin)
    );
    // Pseudo-random bytes, with and without a newline.
    let mut state = 0x2545_F491_4F6C_DD1Du64;
    let mut noise = Vec::new();
    for _ in 0..4096 {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        noise.push((state >> 24) as u8);
    }
    assert!(read_project(&noise).is_err());
}

#[test]
fn a_header_with_a_bad_end_mark_or_version_is_an_error() {
    let mut bytes = sample_bytes();
    let version = stokesforge_import::origin::synthetic::SIGNATURE.len() + 5 + 0x1B;
    bytes[version..version + 8].copy_from_slice(&3.5f64.to_le_bytes());
    assert!(matches!(
        read_project(&bytes),
        Err(OriginError::UnsupportedVersion { .. })
    ));
    let mut bytes = sample_bytes();
    bytes[FIRST_DATASET_OFFSET - 1] = b'x'; // delimiter of the global end mark
    assert!(read_project(&bytes).is_err());
}

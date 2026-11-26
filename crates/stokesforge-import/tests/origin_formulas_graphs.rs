// SPDX-License-Identifier: GPL-3.0-or-later

//! Column formulas and graph curves of the Origin readers (#556, #557), on
//! files written by the synthetic generator, for `.opj` and `.opju` alike.

use stokesforge_import::origin::synthetic::{
    write_opju, write_project, Annotation, Book, Column, GraphCurve, GraphLayer, GraphWindow,
    Project, Sheet,
};
use stokesforge_import::origin::{
    read_project, OriginColumnAt, OriginCurve, OriginDesignation, OriginProject,
};

fn col(name: &str, formula: Option<&str>) -> Column {
    Column {
        formula: formula.map(str::to_string),
        ..Column::numbers(name, OriginDesignation::Y, &[1.0, 2.0])
    }
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
        long_name: String::new(),
        sheets,
        horiba_xml: None,
    }
}

/// The project read from its `.opj` and from its `.opju` (both block-1
/// forms of the `.opju` annotation when `legacy` is not fixed by the caller).
fn read_all(project: &Project) -> Vec<OriginProject> {
    let mut out = vec![read_project(&write_project(project)).unwrap()];
    for legacy in [false, true] {
        let project = Project {
            opju_legacy_formulas: legacy,
            ..project.clone()
        };
        out.push(read_project(&write_opju(&project)).unwrap());
    }
    out
}

fn formulas(project: &OriginProject) -> Vec<Vec<Vec<Option<String>>>> {
    project
        .books
        .iter()
        .map(|b| {
            b.sheets
                .iter()
                .map(|s| s.columns.iter().map(|c| c.formula.clone()).collect())
                .collect()
        })
        .collect()
}

fn at(book: usize, sheet: usize, column: usize) -> Option<OriginColumnAt> {
    Some(OriginColumnAt {
        book,
        sheet,
        column,
    })
}

fn graph(curves: Vec<GraphCurve>) -> GraphWindow {
    GraphWindow {
        short_name: "G1".into(),
        long_name: "Graph one".into(),
        layers: vec![GraphLayer {
            name: "Layer1".into(),
            curves,
        }],
    }
}

fn two_sheets(formula_on_sheet_2: Option<&str>) -> Project {
    Project {
        books: vec![book(
            "Book1",
            vec![
                sheet("S1", vec![col("A", None), col("B", None), col("C", None)]),
                sheet(
                    "S2",
                    vec![col("A", None), col("B", None), col("C", formula_on_sheet_2)],
                ),
            ],
        )],
        ..Project::default()
    }
}

#[test]
fn a_formula_lands_on_its_column_of_its_sheet_only() {
    for read in read_all(&two_sheets(Some("col(A)+col(B)"))) {
        let f = formulas(&read);
        assert_eq!(f[0][1][2].as_deref(), Some("col(A)+col(B)"));
        assert_eq!(f[0][0][2], None, "same name, other sheet");
        assert_eq!(f[0][1][..2], [None, None]);
        assert_eq!(f[0][0][..2], [None, None]);
    }
}

#[test]
fn names_that_are_not_columns_and_blank_texts_give_no_formula() {
    let mut project = two_sheets(None);
    project.books[0].sheets[1].columns[0].formula = Some("   \t ".into());
    for (name, text) in [
        ("SCT", "some script"),
        ("__X", "col(A)"),
        ("Text", "a label"),
        ("SPC_1", "spc"),
        ("", "col(A)"),
        ("Z", "no such column"),
    ] {
        project.annotations.push(Annotation {
            book: 0,
            sheet: 1,
            name: name.into(),
            text: text.into(),
        });
    }
    // One real formula keeps the test honest: only it comes out.
    project.books[0].sheets[1].columns[1].formula = Some("col(A)".into());
    for read in read_all(&project) {
        let f = formulas(&read);
        assert_eq!(f[0][1][1].as_deref(), Some("col(A)"));
        let others = f[0].iter().flatten().filter(|f| f.is_some()).count();
        assert_eq!(others, 1, "only the real formula is attached: {f:?}");
    }
}

#[test]
fn control_characters_and_oversized_texts_give_none_and_whitespace_kept() {
    let mut project = two_sheets(None);
    let long = "x".repeat(64 * 1024 + 1);
    let ok = "a\tb\r\nc".to_string();
    let columns = &mut project.books[0].sheets[0].columns;
    columns[0].formula = Some("bell\u{7}".into());
    columns[1].formula = Some(long);
    columns[2].formula = Some(ok.clone());
    for read in read_all(&project) {
        let f = formulas(&read);
        assert_eq!(f[0][0], [None, None, Some(ok.clone())]);
    }
}

#[test]
fn both_opju_block_one_forms_are_read() {
    let project = two_sheets(Some("col(B)*2"));
    for legacy in [false, true] {
        let project = Project {
            opju_legacy_formulas: legacy,
            ..project.clone()
        };
        let read = read_project(&write_opju(&project)).unwrap();
        assert!(!read.partial);
        assert_eq!(
            read.books[0].sheets[1].columns[2].formula.as_deref(),
            Some("col(B)*2"),
            "legacy form: {legacy}"
        );
    }
}

fn plotted() -> Project {
    Project {
        books: vec![
            book(
                "Book1",
                vec![
                    sheet("S1", vec![col("A", Some("col(B)")), col("B", None)]),
                    sheet("S2", vec![col("A", None), col("B", Some("raw!A"))]),
                ],
            ),
            book("Book2", vec![sheet("S", vec![col("X", None)])]),
        ],
        graphs: vec!["Empty1".into()],
        graph_windows: vec![
            graph(vec![
                GraphCurve::new(2, 1),
                GraphCurve::new(4, 1),
                GraphCurve::new(5, 3),
            ]),
            GraphWindow {
                short_name: "NoLayer".into(),
                long_name: "A note".into(),
                layers: vec![],
            },
        ],
        ..Project::default()
    }
}

#[test]
fn the_opj_and_the_opju_give_the_same_formulas_and_graphs() {
    let reads = read_all(&plotted());
    let opj = &reads[0];
    assert_eq!(formulas(opj)[0][1][1].as_deref(), Some("raw!A"));
    assert_eq!(
        opj.graphs.len(),
        2,
        "Empty1 and G1; the layerless is a note"
    );
    for opju in &reads[1..] {
        assert_eq!(formulas(opju), formulas(opj));
        assert_eq!(opju.graphs, opj.graphs);
        assert_eq!(opju.skipped, opj.skipped);
    }
}

#[test]
fn a_curve_resolves_to_book_sheet_and_column() {
    for read in read_all(&plotted()) {
        assert_eq!(read.skipped.graphs, 3, "windows with no dataset, as before");
        let g = &read.graphs[1];
        assert_eq!(
            (g.short_name.as_str(), g.long_name.as_str()),
            ("G1", "Graph one")
        );
        assert_eq!(g.layers[0].name, "Layer1");
        assert_eq!(
            g.layers[0].curves,
            [
                OriginCurve {
                    y: at(0, 0, 1),
                    x: at(0, 0, 0)
                },
                OriginCurve {
                    y: at(0, 1, 1),
                    x: at(0, 0, 0)
                },
                OriginCurve {
                    y: at(1, 0, 0),
                    x: at(0, 1, 0)
                },
            ]
        );
        // A graph window with a layer and no curve is listed, empty.
        assert_eq!(read.graphs[0].short_name, "Empty1");
        assert!(read.graphs[0].layers[0].curves.is_empty());
    }
}

#[test]
fn a_matrix_dataset_before_the_plotted_one_does_not_shift_the_rank() {
    let project = Project {
        books: vec![book(
            "Book1",
            vec![sheet("S", vec![col("A", None), col("B", None)])],
        )],
        matrices: vec!["M1".into(), "M2".into()],
        matrices_first: true,
        // Ranks: M1 = 1, M2 = 2, A = 3, B = 4.
        graph_windows: vec![graph(vec![
            GraphCurve::new(4, 3),
            GraphCurve::new(1, 3),
            GraphCurve::new(2, 4),
        ])],
        ..Project::default()
    };
    for read in read_all(&project) {
        let curves = &read.graphs[0].layers[0].curves;
        assert_eq!(
            curves[0],
            OriginCurve {
                y: at(0, 0, 1),
                x: at(0, 0, 0)
            }
        );
        // A matrix is not a column of a book.
        assert_eq!(
            curves[1],
            OriginCurve {
                y: None,
                x: at(0, 0, 0)
            }
        );
        assert_eq!(
            curves[2],
            OriginCurve {
                y: None,
                x: at(0, 0, 1)
            }
        );
    }
}

#[test]
fn a_graph_window_before_its_book_resolves() {
    let mut project = plotted();
    project.graph_windows_first = true;
    for read in read_all(&project) {
        assert_eq!(read.books.len(), 2);
        let g = read.graphs.iter().find(|g| g.short_name == "G1").unwrap();
        assert_eq!(g.layers[0].curves[0].y, at(0, 0, 1));
        assert_eq!(g.layers[0].curves[2].y, at(1, 0, 0));
    }
}

#[test]
fn odd_headers_and_indexes_give_none_and_no_panic() {
    let curve = |y, x, header_len| GraphCurve { y, x, header_len };
    let project = Project {
        books: vec![book(
            "Book1",
            vec![sheet("S", vec![col("A", None), col("B", None)])],
        )],
        graph_windows: vec![graph(vec![
            curve(2, 1, 493),
            curve(2, 1, 515),
            // Header too short for the X rank, then for both.
            curve(2, 1, 0x10),
            curve(2, 1, 3),
            curve(0, 1, 515),
            curve(3, 0, 515),
            curve(u16::MAX, 2, 515),
        ])],
        ..Project::default()
    };
    let full = OriginCurve {
        y: at(0, 0, 1),
        x: at(0, 0, 0),
    };
    let none = OriginCurve { y: None, x: None };
    for read in read_all(&project) {
        assert_eq!(
            read.graphs[0].layers[0].curves,
            [
                full,
                full,
                OriginCurve {
                    y: at(0, 0, 1),
                    x: None
                },
                none,
                OriginCurve {
                    y: None,
                    x: at(0, 0, 0)
                },
                OriginCurve { y: None, x: None },
                OriginCurve {
                    y: None,
                    x: at(0, 0, 1)
                },
            ]
        );
    }
}

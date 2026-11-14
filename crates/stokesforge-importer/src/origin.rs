//! Origin project files (`.opj`): from the books read by
//! [`stokesforge_import::origin`] to model columns.
//!
//! The consumers (application, CLI, `pyst`) share this module: detection
//! ([`is_origin_path`], [`is_origin_bytes`]), reading ([`read_origin_file`]),
//! the default choice of sheets ([`default_origin_selection`]), the plan
//! ([`plan_origin_import`]) and the column construction
//! ([`make_origin_column`]). Columns built here carry **no** import source:
//! an Origin column cannot be refreshed from its file.

use std::path::Path;

use stokesforge_core::{Column, ColumnMeta, ColumnRole, MetaLine, MixedCell};
use stokesforge_import::origin::{
    read_project, OriginBook, OriginColumn, OriginDesignation, OriginProject,
};

use crate::{ImportFailure, ImportFailureCause};

/// `true` for a `.opj` or `.opju` extension, whatever its case. `.opju` is
/// routed here too, so the reader gives its clear "not supported" message.
pub fn is_origin_path(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("opj") || ext.eq_ignore_ascii_case("opju"))
}

/// `true` when the content starts with the signature of an Origin file
/// (`CPYA`, or `CPYUA` for `.opju`).
pub fn is_origin_bytes(bytes: &[u8]) -> bool {
    bytes.starts_with(b"CPYA") || bytes.starts_with(b"CPYUA")
}

/// `true` for an Origin file by extension ([`is_origin_path`]) or by content
/// ([`is_origin_bytes`] on the first five bytes). Reads at most five bytes;
/// any I/O error means `false`.
pub fn is_origin_file(path: &Path) -> bool {
    if is_origin_path(path) {
        return true;
    }
    use std::io::Read;
    let mut head = [0u8; 5];
    let Ok(file) = std::fs::File::open(path) else {
        return false;
    };
    let mut len = 0;
    let mut file = file.take(5);
    while len < head.len() {
        match file.read(&mut head[len..]) {
            Ok(0) | Err(_) => break,
            Ok(n) => len += n,
        }
    }
    is_origin_bytes(&head[..len])
}

/// Reads an Origin project from disk.
pub fn read_origin_file(path: &Path) -> Result<OriginProject, ImportFailure> {
    let fail = |cause| ImportFailure {
        path: path.to_path_buf(),
        cause,
    };
    let bytes =
        std::fs::read(path).map_err(|err| fail(ImportFailureCause::Read(err.to_string())))?;
    read_project(&bytes).map_err(|err| fail(ImportFailureCause::Origin(err)))
}

/// Which sheets are ticked: per book, per sheet.
pub type OriginSelection = Vec<Vec<bool>>;

/// Per book, the sheets named `Data` (ASCII case-insensitive); a book with no
/// such sheet ticks every sheet that has at least one column.
pub fn default_origin_selection(project: &OriginProject) -> OriginSelection {
    project
        .books
        .iter()
        .map(|book| {
            let has_data = book
                .sheets
                .iter()
                .any(|sheet| sheet.name.eq_ignore_ascii_case("Data"));
            book.sheets
                .iter()
                .map(|sheet| {
                    if has_data {
                        sheet.name.eq_ignore_ascii_case("Data")
                    } else {
                        !sheet.columns.is_empty()
                    }
                })
                .collect()
        })
        .collect()
}

fn non_blank<'a>(preferred: &'a str, fallback: &'a str) -> &'a str {
    let preferred = preferred.trim();
    if preferred.is_empty() {
        fallback
    } else {
        preferred
    }
}

/// The name of a book: the first non-blank line of its long name (Horiba
/// labels hold a second line, such as `Spectral Acquisition[Emission]`, that a
/// workbook name cannot carry), else its short name.
pub fn origin_book_name(book: &OriginBook) -> &str {
    let first_line = book
        .long_name
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or("");
    non_blank(first_line, &book.short_name)
}

/// A sheet to create.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OriginSheetPlan {
    /// Index of the sheet in its book.
    pub sheet: usize,
    pub name: String,
}

/// A workbook to create, with the sheets to fill.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OriginBookPlan {
    /// Index of the book in the project.
    pub book: usize,
    pub name: String,
    pub sheets: Vec<OriginSheetPlan>,
}

/// The books and sheets to create for `selection`, in file order. A book with
/// no ticked sheet is omitted; a selection shorter than the project counts
/// as unticked.
pub fn plan_origin_import(
    project: &OriginProject,
    selection: &OriginSelection,
) -> Vec<OriginBookPlan> {
    project
        .books
        .iter()
        .enumerate()
        .filter_map(|(book_index, book)| {
            let ticks = selection.get(book_index);
            let sheets: Vec<OriginSheetPlan> = book
                .sheets
                .iter()
                .enumerate()
                .filter(|(sheet_index, _)| {
                    ticks.is_some_and(|t| t.get(*sheet_index).copied().unwrap_or(false))
                })
                .map(|(sheet, origin)| OriginSheetPlan {
                    sheet,
                    name: origin.name.clone(),
                })
                .collect();
            (!sheets.is_empty()).then(|| OriginBookPlan {
                book: book_index,
                name: origin_book_name(book).to_string(),
                sheets,
            })
        })
        .collect()
}

/// Builds the model column for an Origin column: numeric, or mixed as soon as
/// a cell is text; long name, unit, comment, role and the book's Horiba
/// information as file metadata. No import source.
pub fn make_origin_column(book: &OriginBook, column: &OriginColumn) -> Column {
    let mut built = if column.has_text() {
        let cells: Vec<MixedCell> = column
            .values
            .iter()
            .enumerate()
            .map(|(row, &value)| match column.text(row) {
                Some(text) => MixedCell::Text(text.to_string()),
                None => MixedCell::from_f64(value),
            })
            .collect();
        Column::from_mixed(&cells)
    } else {
        Column::from_f64(&column.values)
    };
    let role = match column.designation {
        OriginDesignation::X => ColumnRole::X,
        OriginDesignation::Y => ColumnRole::Y,
        OriginDesignation::Z => ColumnRole::Z,
        OriginDesignation::XError => ColumnRole::XError,
        OriginDesignation::YError => ColumnRole::YError,
        OriginDesignation::Label => ColumnRole::Label,
        OriginDesignation::Disregard => ColumnRole::None,
    };
    built.set_meta(ColumnMeta {
        long_name: non_blank(&column.long_name, &column.short_name).to_string(),
        unit: column.unit.clone(),
        comment: column.comment.clone(),
        file_metadata: book
            .info
            .iter()
            .map(|line| MetaLine {
                key: line.key.clone(),
                value: line.value.clone(),
            })
            .collect(),
        role,
        import_source: None,
        ..Default::default()
    });
    built
}

#[cfg(test)]
mod tests {
    use super::*;
    use stokesforge_core::ColumnType;
    use stokesforge_import::origin::synthetic::{self, Cell};
    use stokesforge_import::origin::OriginError;

    fn sheet(name: &str, columns: Vec<synthetic::Column>) -> synthetic::Sheet {
        synthetic::Sheet {
            name: name.into(),
            columns,
        }
    }

    fn col(short: &str, designation: OriginDesignation) -> synthetic::Column {
        synthetic::Column::numbers(short, designation, &[1.0, 2.0])
    }

    fn book(short: &str, long: &str, sheets: Vec<synthetic::Sheet>) -> synthetic::Book {
        synthetic::Book {
            short_name: short.into(),
            long_name: long.into(),
            sheets,
            horiba_xml: None,
        }
    }

    fn read(books: Vec<synthetic::Book>) -> OriginProject {
        let bytes = synthetic::write_project(&synthetic::Project {
            books,
            ..synthetic::Project::default()
        });
        read_project(&bytes).expect("synthetic project")
    }

    #[test]
    fn every_designation_maps_to_its_role() {
        let designations = [
            (OriginDesignation::X, ColumnRole::X),
            (OriginDesignation::Y, ColumnRole::Y),
            (OriginDesignation::Z, ColumnRole::Z),
            (OriginDesignation::XError, ColumnRole::XError),
            (OriginDesignation::YError, ColumnRole::YError),
            (OriginDesignation::Label, ColumnRole::Label),
            (OriginDesignation::Disregard, ColumnRole::None),
        ];
        let columns = designations
            .iter()
            .enumerate()
            .map(|(i, (d, _))| col(&format!("C{i}"), *d))
            .collect();
        let project = read(vec![book("B", "", vec![sheet("Data", columns)])]);
        let origin_book = &project.books[0];
        let sheet = &origin_book.sheets[0];
        assert_eq!(sheet.columns.len(), 7);
        for (column, (designation, role)) in sheet.columns.iter().zip(designations) {
            assert_eq!(column.designation, designation);
            let built = make_origin_column(origin_book, column);
            assert_eq!(built.meta().role, role, "{designation:?}");
            assert!(built.meta().import_source.is_none());
        }
    }

    #[test]
    fn horiba_block_is_carried_per_book_only() {
        let mut a = book(
            "A",
            "",
            vec![sheet("Data", vec![col("X", OriginDesignation::X)])],
        );
        a.horiba_xml =
            Some("<HJYInfo><ExpSummary><SWVersion>3.0</SWVersion></ExpSummary></HJYInfo>".into());
        let b = book(
            "B",
            "",
            vec![sheet("Data", vec![col("X", OriginDesignation::X)])],
        );
        let project = read(vec![a, b]);
        let in_a = make_origin_column(&project.books[0], &project.books[0].sheets[0].columns[0]);
        let in_b = make_origin_column(&project.books[1], &project.books[1].sheets[0].columns[0]);
        assert_eq!(
            in_a.meta().file_metadata,
            [MetaLine {
                key: "HJYInfo.ExpSummary.SWVersion".into(),
                value: "3.0".into()
            }]
        );
        assert!(in_b.meta().file_metadata.is_empty());
    }

    #[test]
    fn one_text_cell_makes_the_column_mixed() {
        let mut text = col("T", OriginDesignation::Y);
        text.cells = vec![Cell::Number(1.0), Cell::Text("n/a".into()), Cell::Missing];
        let project = read(vec![book("B", "", vec![sheet("Data", vec![text])])]);
        let origin_book = &project.books[0];
        let built = make_origin_column(origin_book, &origin_book.sheets[0].columns[0]);
        assert_eq!(built.column_type(), ColumnType::Mixed);
        let numeric = make_origin_column(origin_book, &col_read(&[1.0, 2.0]));
        assert_eq!(numeric.column_type(), ColumnType::Numeric);
    }

    fn col_read(values: &[f64]) -> OriginColumn {
        OriginColumn {
            short_name: "N".into(),
            designation: OriginDesignation::Y,
            long_name: String::new(),
            unit: String::new(),
            comment: String::new(),
            values: values.to_vec(),
            texts: Vec::new(),
        }
    }

    #[test]
    fn default_selection_prefers_data_sheets_per_book() {
        let empty = sheet("Empty", vec![]);
        let project = read(vec![
            book(
                "A",
                "",
                vec![
                    sheet("Note", vec![col("A", OriginDesignation::Y)]),
                    sheet("data", vec![col("A", OriginDesignation::Y)]),
                    empty.clone(),
                ],
            ),
            book(
                "B",
                "",
                vec![
                    sheet("Raw", vec![col("A", OriginDesignation::Y)]),
                    empty,
                    sheet("Fit", vec![col("A", OriginDesignation::Y)]),
                ],
            ),
        ]);
        assert_eq!(
            default_origin_selection(&project),
            vec![vec![false, true, false], vec![true, false, true]]
        );
    }

    #[test]
    fn plan_skips_unticked_books_and_tolerates_a_short_selection() {
        let one = |n: &str| {
            book(
                n,
                "",
                vec![
                    sheet("S", vec![col("A", OriginDesignation::Y)]),
                    sheet("T", vec![col("A", OriginDesignation::Y)]),
                ],
            )
        };
        let project = read(vec![one("A"), one("B"), one("C")]);
        let plan = plan_origin_import(&project, &vec![vec![false, true], vec![false, false]]);
        assert_eq!(
            plan,
            [OriginBookPlan {
                book: 0,
                name: "A".into(),
                sheets: vec![OriginSheetPlan {
                    sheet: 1,
                    name: "T".into()
                }],
            }]
        );
        // Shorter rows and a missing row count as unticked.
        assert!(plan_origin_import(&project, &vec![vec![true]]).len() == 1);
        assert!(plan_origin_import(&project, &Vec::new()).is_empty());
    }

    #[test]
    fn blank_long_names_fall_back_to_the_short_ones() {
        let mut named = col("A", OriginDesignation::Y);
        named.long_name = "Signal".into();
        let mut blank = col("B", OriginDesignation::Y);
        blank.long_name = "   ".into();
        let project = read(vec![
            book("Book1", "  ", vec![sheet("Data", vec![named, blank])]),
            book(
                "Book2",
                "Long",
                vec![sheet("Data", vec![col("A", OriginDesignation::Y)])],
            ),
        ]);
        assert_eq!(origin_book_name(&project.books[0]), "Book1");
        assert_eq!(origin_book_name(&project.books[1]), "Long");
        let two_lines = read(vec![book(
            "Book3",
            "  \r\nSample A\r\nSpectral Acquisition[Emission]",
            vec![sheet("Data", vec![col("A", OriginDesignation::Y)])],
        )]);
        assert_eq!(origin_book_name(&two_lines.books[0]), "Sample A");
        let columns = &project.books[0].sheets[0].columns;
        let name = |c: &OriginColumn| {
            make_origin_column(&project.books[0], c)
                .meta()
                .long_name
                .clone()
        };
        assert_eq!(name(&columns[0]), "Signal");
        assert_eq!(name(&columns[1]), "B");
    }

    #[test]
    fn origin_files_are_recognised_by_extension_and_signature() {
        for yes in ["a.opj", "A.OPJ", "dir/x.Opju", "x.opju"] {
            assert!(is_origin_path(Path::new(yes)), "{yes}");
        }
        for no in ["a.csv", "opj", "a.opj.txt", "a"] {
            assert!(!is_origin_path(Path::new(no)), "{no}");
        }
        assert!(is_origin_bytes(b"CPYA 4.3268 195 W64 #\n"));
        assert!(is_origin_bytes(b"CPYUA 5 #\n"));
        assert!(!is_origin_bytes(b"CPY"));
        assert!(!is_origin_bytes(b"wavelength;abs"));
    }

    #[test]
    fn read_origin_file_reports_the_file_and_the_cause() {
        let dir = std::env::temp_dir().join(format!("sf-origin-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("bad.opj");
        std::fs::write(&path, b"not an origin file\n").unwrap();
        let failure = read_origin_file(&path).unwrap_err();
        assert_eq!(failure.path, path);
        assert_eq!(
            failure.cause,
            ImportFailureCause::Origin(OriginError::NotOrigin)
        );
        let missing = read_origin_file(&dir.join("absent.opj")).unwrap_err();
        assert!(matches!(missing.cause, ImportFailureCause::Read(_)));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn origin_file_is_recognised_by_extension_or_content() {
        let dir = std::env::temp_dir().join(format!("sf-origin-file-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let write = |name: &str, bytes: &[u8]| {
            let path = dir.join(name);
            std::fs::write(&path, bytes).unwrap();
            path
        };
        assert!(is_origin_file(&write("measure.dat", b"CPYA 4.2673 rest")));
        assert!(is_origin_file(&write("unicode.bin", b"CPYUA 5.0")));
        assert!(is_origin_file(&write("text.opj", b"1,2\n3,4\n")));
        assert!(!is_origin_file(&write("text.dat", b"1,2\n3,4\n")));
        assert!(!is_origin_file(&dir.join("absent.dat")));
        assert!(!is_origin_file(&write("empty.dat", b"")));
        assert!(!is_origin_file(&write("short.dat", b"CPY")));
        std::fs::remove_dir_all(&dir).ok();
    }
}

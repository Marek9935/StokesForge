//! Origin project files (`.opj`, `.opju`): from the books read by
//! [`stokesforge_import::origin`] to model columns.
//!
//! The consumers (application, CLI, `pyst`) share this module: detection
//! ([`is_origin_path`], [`is_origin_bytes`]), reading ([`read_origin_file`]),
//! the default choice of sheets ([`default_origin_selection`]), the plan
//! ([`plan_origin_import`]) and the column construction
//! ([`make_origin_column`]). Columns built here carry **no** import source:
//! an Origin column cannot be refreshed from its file.

use std::collections::BTreeMap;
use std::path::Path;

use stokesforge_core::{Column, ColumnMeta, ColumnRole, MetaLine, MixedCell};
use stokesforge_import::origin::{
    read_project, OriginBook, OriginColumn, OriginDesignation, OriginProject,
};

use crate::planner::{plan_tables, same_x_values, PlanColumn, PlanTable};
use crate::PlannedColumn;
use crate::{ImportFailure, ImportFailureCause};
use stokesforge_import::FilesLayout;

/// `true` for a `.opj` or `.opju` extension, whatever its case.
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

/// Which columns are ticked: per book, per sheet, per column.
pub type OriginSelection = Vec<Vec<Vec<bool>>>;

/// Per book, the sheets named `Data` (ASCII case-insensitive); a book with no
/// such sheet ticks every sheet that has at least one column. A ticked sheet
/// has all its columns ticked.
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
                    let ticked = if has_data {
                        sheet.name.eq_ignore_ascii_case("Data")
                    } else {
                        !sheet.columns.is_empty()
                    };
                    vec![ticked; sheet.columns.len()]
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

/// Where the workbooks of an Origin import go: the "Where" of the classic
/// import, without "Active Workbook" (#537).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OriginWhere {
    /// One workbook per Origin workbook (what the import has always done).
    #[default]
    PerOriginWorkbook,
    /// A single new workbook for everything.
    NewWorkbook,
}

/// One Origin column to copy: file (rank among the sources), book, sheet and
/// column ranks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OriginColumnRef {
    pub file: usize,
    pub book: usize,
    pub sheet: usize,
    pub column: usize,
    /// Another column of the same new sheet, from another Origin sheet, has the
    /// same name: the Origin sheet name goes in this column's Comments.
    pub tag_sheet: bool,
}

/// An Origin column by rank: (file rank among the sources, book, sheet,
/// column). The file is part of the key, two files hold the same ranks.
pub type OriginColumnKey = (usize, usize, usize, usize);

impl OriginColumnRef {
    /// The rank key of this column.
    pub fn key(&self) -> OriginColumnKey {
        (self.file, self.book, self.sheet, self.column)
    }
}

/// An Origin sheet, as the plan names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OriginSheetRef {
    pub file: usize,
    pub book: usize,
    pub sheet: usize,
}

/// A sheet to create.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OriginSheetPlan {
    /// Index of the (first) Origin sheet in its book.
    pub sheet: usize,
    pub name: String,
    /// The columns it receives, in order.
    pub columns: Vec<OriginColumnRef>,
}

/// A workbook to create, with the sheets to fill.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OriginBookPlan {
    /// Index of the file among the sources (always 0 for one project).
    pub file: usize,
    /// Index of the (first) book in the project.
    pub book: usize,
    /// Empty: the model gives the automatic name.
    pub name: String,
    pub sheets: Vec<OriginSheetPlan>,
}

/// What [`plan_origin_layout`] says.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct OriginLayoutPlan {
    pub books: Vec<OriginBookPlan>,
    /// Shared X only: the Origin sheets that keep an X of their own because its
    /// values differ from the first X (data: the window phrases it).
    pub own_x: Vec<OriginSheetRef>,
    /// Shared X only: for every ticked X column, the ticked X column whose
    /// values the new sheet keeps. A kept X (the first, or one of different
    /// values) maps to itself; a dropped X (same values) maps to the first X.
    /// Empty in the other layouts: no X is dropped there.
    pub x_map: BTreeMap<OriginColumnKey, OriginColumnKey>,
}

/// One Origin project and its ticks, for [`plan_origin_layout`].
#[derive(Debug, Clone, Copy)]
pub struct OriginSource<'a> {
    pub project: &'a OriginProject,
    pub selection: &'a OriginSelection,
    /// The file name without its extension: names the workbook when it is the
    /// only source of a [`OriginWhere::NewWorkbook`].
    pub file_stem: &'a str,
}

fn is_ticked(selection: &OriginSelection, book: usize, sheet: usize, column: usize) -> bool {
    selection
        .get(book)
        .and_then(|b| b.get(sheet))
        .and_then(|s| s.get(column))
        .copied()
        .unwrap_or(false)
}

/// The name that decides "same column" in the all-sheets gesture and in the
/// Comments rule: the long name, else the short name.
pub fn origin_column_key(column: &OriginColumn) -> &str {
    non_blank(&column.long_name, &column.short_name)
}

/// The books and sheets to create for the ticked columns, laid out like the
/// classic import: where (`where_`) and how the Origin sheets (one table each)
/// share worksheets (`layout`). A sheet with no ticked column is no table. In
/// a shared-X layout the first X stays and an equal later X is dropped
/// ([`crate::planner::same_x_values`]).
pub fn plan_origin_layout(
    sources: &[OriginSource<'_>],
    where_: OriginWhere,
    layout: FilesLayout,
) -> OriginLayoutPlan {
    let mut tables: Vec<(OriginSheetRef, PlanTable)> = Vec::new();
    for (file, source) in sources.iter().enumerate() {
        for (book, origin_book) in source.project.books.iter().enumerate() {
            for (sheet, origin_sheet) in origin_book.sheets.iter().enumerate() {
                let columns: Vec<PlanColumn> = origin_sheet
                    .columns
                    .iter()
                    .enumerate()
                    .map(|(column, c)| PlanColumn {
                        is_x: c.designation == OriginDesignation::X,
                        kept: is_ticked(source.selection, book, sheet, column),
                    })
                    .collect();
                if columns.iter().any(|c| c.kept) {
                    tables.push((
                        OriginSheetRef { file, book, sheet },
                        PlanTable {
                            name: origin_sheet.name.clone(),
                            columns,
                            readable: true,
                        },
                    ));
                }
            }
        }
    }
    // The groups of tables that become workbooks.
    let mut groups: Vec<Vec<usize>> = Vec::new();
    match where_ {
        OriginWhere::NewWorkbook => {
            if !tables.is_empty() {
                groups.push((0..tables.len()).collect());
            }
        }
        OriginWhere::PerOriginWorkbook => {
            for (index, (at, _)) in tables.iter().enumerate() {
                match groups.last_mut() {
                    Some(group)
                        if tables[group[0]].0.file == at.file
                            && tables[group[0]].0.book == at.book =>
                    {
                        group.push(index);
                    }
                    _ => groups.push(vec![index]),
                }
            }
        }
    }
    let column_of = |at: OriginSheetRef, column: usize| {
        &sources[at.file].project.books[at.book].sheets[at.sheet].columns[column]
    };
    let mut out = OriginLayoutPlan::default();
    for group in groups {
        let members: Vec<PlanTable> = group.iter().map(|&i| tables[i].1.clone()).collect();
        let at = |item: usize| tables[group[item]].0;
        let plans = plan_tables(&members, layout, &|a, b| {
            same_x_values(
                &column_of(at(a.item), a.column).values,
                &column_of(at(b.item), b.column).values,
            )
        });
        let first = at(0);
        let name = match where_ {
            OriginWhere::PerOriginWorkbook => {
                origin_book_name(&sources[first.file].project.books[first.book]).to_string()
            }
            OriginWhere::NewWorkbook if sources.len() == 1 => sources[0].file_stem.to_string(),
            OriginWhere::NewWorkbook => String::new(),
        };
        let sheets = plans
            .iter()
            .map(|plan| {
                let mut columns: Vec<OriginColumnRef> = plan
                    .columns
                    .iter()
                    .map(|planned| {
                        let from = at(planned.item);
                        OriginColumnRef {
                            file: from.file,
                            book: from.book,
                            sheet: from.sheet,
                            column: planned.column,
                            tag_sheet: false,
                        }
                    })
                    .collect();
                // The same long name from several Origin sheets side by side.
                let sheet_of = |c: &OriginColumnRef| (c.file, c.book, c.sheet);
                let tagged: Vec<bool> = columns
                    .iter()
                    .map(|c| {
                        let key = origin_column_key(column_of(
                            OriginSheetRef {
                                file: c.file,
                                book: c.book,
                                sheet: c.sheet,
                            },
                            c.column,
                        ));
                        columns.iter().any(|other| {
                            sheet_of(other) != sheet_of(c)
                                && origin_column_key(column_of(
                                    OriginSheetRef {
                                        file: other.file,
                                        book: other.book,
                                        sheet: other.sheet,
                                    },
                                    other.column,
                                )) == key
                        })
                    })
                    .collect();
                for (column, tag) in columns.iter_mut().zip(tagged) {
                    column.tag_sheet = tag;
                }
                OriginSheetPlan {
                    sheet: at(plan.items[0]).sheet,
                    name: plan.name.clone(),
                    columns,
                }
            })
            .collect();
        for plan in &plans {
            for &(column, kept) in &plan.x_map {
                let key = |c: PlannedColumn| {
                    let from = at(c.item);
                    (from.file, from.book, from.sheet, c.column)
                };
                out.x_map.insert(key(column), key(kept));
            }
            for &item in &plan.own_x {
                let sheet = at(item);
                if !out.own_x.contains(&sheet) {
                    out.own_x.push(sheet);
                }
            }
        }
        out.books.push(OriginBookPlan {
            file: first.file,
            book: first.book,
            name,
            sheets,
        });
    }
    out
}

/// The books and sheets to create for `selection`, one workbook per Origin
/// workbook and one sheet per Origin sheet, in file order. A book with no
/// ticked column is omitted; a selection shorter than the project counts as
/// unticked. The default of every consumer (CLI, `pyst`).
pub fn plan_origin_import(
    project: &OriginProject,
    selection: &OriginSelection,
) -> Vec<OriginBookPlan> {
    plan_origin_layout(
        &[OriginSource {
            project,
            selection,
            file_stem: "",
        }],
        OriginWhere::PerOriginWorkbook,
        FilesLayout::SheetPerFile,
    )
    .books
}

/// Builds the model column for an Origin column: numeric, or mixed as soon as
/// a cell is text; long name, unit, comment, role and the book's Horiba
/// information as file metadata. No import source.
pub fn make_origin_column(book: &OriginBook, column: &OriginColumn) -> Column {
    make_origin_column_in(book, column, None)
}

/// [`make_origin_column`], for a column of a combined sheet: with
/// `sheet_name`, the Origin sheet name goes in the Comments (before the
/// column's own comment, when it has one).
pub fn make_origin_column_in(
    book: &OriginBook,
    column: &OriginColumn,
    sheet_name: Option<&str>,
) -> Column {
    let comment = match sheet_name {
        Some(sheet) if column.comment.trim().is_empty() => sheet.to_string(),
        Some(sheet) => format!("{sheet}: {}", column.comment),
        None => column.comment.clone(),
    };
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
        comment,
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
            formula: None,
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
            vec![
                vec![vec![false], vec![true], vec![]],
                vec![vec![true], vec![], vec![true]]
            ]
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
        let plan = plan_origin_import(
            &project,
            &vec![
                vec![vec![false], vec![true]],
                vec![vec![false], vec![false]],
            ],
        );
        assert_eq!(
            plan,
            [OriginBookPlan {
                file: 0,
                book: 0,
                name: "A".into(),
                sheets: vec![OriginSheetPlan {
                    sheet: 1,
                    name: "T".into(),
                    columns: vec![OriginColumnRef {
                        file: 0,
                        book: 0,
                        sheet: 1,
                        column: 0,
                        tag_sheet: false
                    }],
                }],
            }]
        );
        // Shorter rows and a missing row count as unticked.
        assert!(plan_origin_import(&project, &vec![vec![vec![true]]]).len() == 1);
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

    fn named(short: &str, long: &str, d: OriginDesignation, v: &[f64]) -> synthetic::Column {
        let mut c = synthetic::Column::numbers(short, d, v);
        c.long_name = long.into();
        c
    }

    fn xy_sheet(name: &str, x: &[f64], long: &str) -> synthetic::Sheet {
        sheet(
            name,
            vec![
                named("X", "Time", OriginDesignation::X, x),
                named("Y", long, OriginDesignation::Y, &[7.0, 8.0]),
            ],
        )
    }

    fn all_ticked(project: &OriginProject) -> OriginSelection {
        project
            .books
            .iter()
            .map(|b| {
                b.sheets
                    .iter()
                    .map(|s| vec![true; s.columns.len()])
                    .collect()
            })
            .collect()
    }

    fn refs(sheet: &OriginSheetPlan) -> Vec<(usize, usize, usize)> {
        sheet
            .columns
            .iter()
            .map(|c| (c.book, c.sheet, c.column))
            .collect()
    }

    #[test]
    fn the_default_layout_is_what_the_import_always_did() {
        let project = read(vec![
            book(
                "A",
                "Long",
                vec![
                    xy_sheet("Note", &[1.0, 2.0], "n"),
                    xy_sheet("Data", &[1.0, 2.0], "d"),
                ],
            ),
            book("B", "", vec![xy_sheet("One", &[1.0, 2.0], "o")]),
        ]);
        let selection = default_origin_selection(&project);
        let source = OriginSource {
            project: &project,
            selection: &selection,
            file_stem: "f",
        };
        let plan = plan_origin_layout(
            &[source],
            OriginWhere::PerOriginWorkbook,
            FilesLayout::SheetPerFile,
        );
        assert_eq!(plan.books, plan_origin_import(&project, &selection));
        assert!(plan.own_x.is_empty());
        let names: Vec<(&str, Vec<&str>)> = plan
            .books
            .iter()
            .map(|b| {
                (
                    b.name.as_str(),
                    b.sheets.iter().map(|s| s.name.as_str()).collect(),
                )
            })
            .collect();
        assert_eq!(names, [("Long", vec!["Data"]), ("B", vec!["One"])]);
        assert_eq!(refs(&plan.books[0].sheets[0]), [(0, 1, 0), (0, 1, 1)]);
    }

    #[test]
    fn per_workbook_with_a_one_sheet_layout_gives_one_combined_sheet_per_workbook() {
        let project = read(vec![
            book(
                "A",
                "",
                vec![
                    xy_sheet("First", &[1.0, 2.0], "a"),
                    xy_sheet("Second", &[1.0, 2.0], "b"),
                ],
            ),
            book("B", "", vec![xy_sheet("Third", &[5.0, 6.0], "c")]),
        ]);
        let selection = all_ticked(&project);
        let source = OriginSource {
            project: &project,
            selection: &selection,
            file_stem: "f",
        };
        for layout in [FilesLayout::SideBySide, FilesLayout::SharedX] {
            let plan = plan_origin_layout(&[source], OriginWhere::PerOriginWorkbook, layout);
            assert_eq!(plan.books.len(), 2, "{layout:?}");
            assert_eq!(plan.books[0].sheets.len(), 1);
            assert_eq!(plan.books[0].sheets[0].name, "First", "first sheet's name");
            assert_eq!(plan.books[1].sheets[0].name, "Third");
        }
    }

    #[test]
    fn a_new_workbook_holds_everything_and_is_named_after_a_single_file() {
        let project = read(vec![
            book("A", "", vec![xy_sheet("First", &[1.0, 2.0], "a")]),
            book("B", "", vec![xy_sheet("Second", &[1.0, 2.0], "b")]),
        ]);
        let selection = all_ticked(&project);
        let source = OriginSource {
            project: &project,
            selection: &selection,
            file_stem: "kinetics",
        };
        let one = plan_origin_layout(
            &[source],
            OriginWhere::NewWorkbook,
            FilesLayout::SheetPerFile,
        );
        assert_eq!(one.books.len(), 1);
        assert_eq!(one.books[0].name, "kinetics");
        assert_eq!(one.books[0].sheets.len(), 2);
        let two = plan_origin_layout(
            &[source, source],
            OriginWhere::NewWorkbook,
            FilesLayout::SheetPerFile,
        );
        assert_eq!(two.books.len(), 1);
        assert_eq!(two.books[0].name, "", "several files: automatic name");
        assert_eq!(two.books[0].sheets.len(), 4);
    }

    #[test]
    fn shared_x_drops_an_equal_x_and_keeps_and_reports_a_different_one() {
        let project = read(vec![book(
            "A",
            "",
            vec![
                xy_sheet("One", &[1.0, 2.0], "a"),
                xy_sheet("Two", &[1.0, 2.0], "b"),
                xy_sheet("Three", &[1.0, 3.0], "c"),
            ],
        )]);
        let selection = all_ticked(&project);
        let source = OriginSource {
            project: &project,
            selection: &selection,
            file_stem: "f",
        };
        let plan = plan_origin_layout(
            &[source],
            OriginWhere::PerOriginWorkbook,
            FilesLayout::SharedX,
        );
        let sheet = &plan.books[0].sheets[0];
        assert_eq!(
            refs(sheet),
            [(0, 0, 0), (0, 0, 1), (0, 1, 1), (0, 2, 0), (0, 2, 1)],
            "X0 Y0 Y1 X2 Y2"
        );
        assert_eq!(
            plan.own_x,
            [OriginSheetRef {
                file: 0,
                book: 0,
                sheet: 2
            }]
        );
    }

    #[test]
    fn shared_x_maps_a_dropped_x_to_the_kept_one_and_a_different_x_to_itself() {
        let project = read(vec![book(
            "A",
            "",
            vec![
                xy_sheet("One", &[1.0, 2.0], "a"),
                xy_sheet("Two", &[1.0, 2.0], "b"),
                xy_sheet("Three", &[1.0, 3.0], "c"),
            ],
        )]);
        let selection = all_ticked(&project);
        let source = OriginSource {
            project: &project,
            selection: &selection,
            file_stem: "f",
        };
        let plan = plan_origin_layout(
            &[source],
            OriginWhere::PerOriginWorkbook,
            FilesLayout::SharedX,
        );
        let expected: BTreeMap<OriginColumnKey, OriginColumnKey> = BTreeMap::from([
            ((0, 0, 0, 0), (0, 0, 0, 0)),
            ((0, 0, 1, 0), (0, 0, 0, 0)),
            ((0, 0, 2, 0), (0, 0, 2, 0)),
        ]);
        assert_eq!(plan.x_map, expected);
        let kept: Vec<OriginColumnKey> = plan.books[0].sheets[0]
            .columns
            .iter()
            .map(OriginColumnRef::key)
            .collect();
        assert!(
            !kept.contains(&(0, 0, 1, 0)),
            "the dropped X is not created"
        );
    }

    #[test]
    fn side_by_side_and_one_sheet_per_file_drop_no_x() {
        let project = read(vec![book(
            "A",
            "",
            vec![
                xy_sheet("One", &[1.0, 2.0], "a"),
                xy_sheet("Two", &[1.0, 2.0], "b"),
            ],
        )]);
        let selection = all_ticked(&project);
        for layout in [FilesLayout::SideBySide, FilesLayout::SheetPerFile] {
            let plan = plan_origin_layout(
                &[OriginSource {
                    project: &project,
                    selection: &selection,
                    file_stem: "f",
                }],
                OriginWhere::PerOriginWorkbook,
                layout,
            );
            assert!(plan.x_map.is_empty(), "{layout:?}");
        }
    }

    #[test]
    fn shared_x_keys_of_two_files_carry_the_file() {
        let one = read(vec![book("A", "", vec![xy_sheet("S", &[1.0, 2.0], "a")])]);
        let two = read(vec![book("A", "", vec![xy_sheet("S", &[1.0, 2.0], "b")])]);
        let (sel_one, sel_two) = (all_ticked(&one), all_ticked(&two));
        let plan = plan_origin_layout(
            &[
                OriginSource {
                    project: &one,
                    selection: &sel_one,
                    file_stem: "one",
                },
                OriginSource {
                    project: &two,
                    selection: &sel_two,
                    file_stem: "two",
                },
            ],
            OriginWhere::NewWorkbook,
            FilesLayout::SharedX,
        );
        let expected: BTreeMap<OriginColumnKey, OriginColumnKey> =
            BTreeMap::from([((0, 0, 0, 0), (0, 0, 0, 0)), ((1, 0, 0, 0), (0, 0, 0, 0))]);
        assert_eq!(plan.x_map, expected);
    }

    #[test]
    fn shared_x_compares_a_second_x_of_the_same_sheet() {
        let mut columns = vec![
            named("X1", "Time", OriginDesignation::X, &[1.0, 2.0]),
            named("Y1", "a", OriginDesignation::Y, &[7.0, 8.0]),
            named("X2", "Time", OriginDesignation::X, &[1.0, 2.0]),
            named("Y2", "b", OriginDesignation::Y, &[7.0, 8.0]),
        ];
        let project = read(vec![book("A", "", vec![sheet("S", columns.clone())])]);
        let selection = all_ticked(&project);
        let plan_of = |project: &OriginProject, selection: &OriginSelection| {
            plan_origin_layout(
                &[OriginSource {
                    project,
                    selection,
                    file_stem: "f",
                }],
                OriginWhere::PerOriginWorkbook,
                FilesLayout::SharedX,
            )
        };
        let plan = plan_of(&project, &selection);
        assert_eq!(
            refs(&plan.books[0].sheets[0]),
            [(0, 0, 0), (0, 0, 1), (0, 0, 3)]
        );
        columns[2] = named("X2", "Time", OriginDesignation::X, &[1.0, 9.0]);
        let project = read(vec![book("A", "", vec![sheet("S", columns)])]);
        let plan = plan_of(&project, &all_ticked(&project));
        assert_eq!(plan.books[0].sheets[0].columns.len(), 4);
        assert_eq!(plan.own_x.len(), 1);
    }

    #[test]
    fn unticked_columns_are_absent_and_a_y_without_its_x_is_allowed() {
        let project = read(vec![book(
            "A",
            "",
            vec![
                xy_sheet("One", &[1.0, 2.0], "a"),
                xy_sheet("Two", &[1.0, 2.0], "b"),
            ],
        )]);
        // Only the Y of the first sheet; nothing of the second.
        let selection = vec![vec![vec![false, true], vec![false, false]]];
        let plan = plan_origin_import(&project, &selection);
        assert_eq!(plan.len(), 1);
        assert_eq!(
            plan[0].sheets.len(),
            1,
            "a sheet with nothing ticked is no table"
        );
        assert_eq!(refs(&plan[0].sheets[0]), [(0, 0, 1)]);
    }

    #[test]
    fn the_same_long_name_from_two_sheets_side_by_side_gets_the_sheet_name_in_comments() {
        let project = read(vec![book(
            "A",
            "",
            vec![
                xy_sheet("Run1", &[1.0, 2.0], "Signal"),
                xy_sheet("Run2", &[1.0, 2.0], "Signal"),
                xy_sheet("Run3", &[1.0, 2.0], "Other"),
            ],
        )]);
        let selection = all_ticked(&project);
        let plan = plan_origin_layout(
            &[OriginSource {
                project: &project,
                selection: &selection,
                file_stem: "f",
            }],
            OriginWhere::NewWorkbook,
            FilesLayout::SideBySide,
        );
        let tagged: Vec<(usize, usize)> = plan.books[0].sheets[0]
            .columns
            .iter()
            .filter(|c| c.tag_sheet)
            .map(|c| (c.sheet, c.column))
            .collect();
        // The three X are all "Time" (sheets differ): tagged too; Y of Run3 is not.
        assert_eq!(tagged, [(0, 0), (0, 1), (1, 0), (1, 1), (2, 0)]);
        let book = &project.books[0];
        let built = make_origin_column_in(book, &book.sheets[0].columns[1], Some("Run1"));
        assert_eq!(built.meta().comment, "Run1");
        let plain = make_origin_column_in(book, &book.sheets[0].columns[1], None);
        assert_eq!(plain.meta().comment, "");
    }
}

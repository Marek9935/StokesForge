//! The one path from import plans to workbooks, sheets and columns (#537).
//!
//! The classic import ([`AppState::apply_import`](super::AppState)) and the
//! Origin import (`import_origin`) both turn their plans into the same
//! description ([`BookApply`]) and run [`AppState::apply_book_plans`]. What
//! differs stays in two small switches: how names are given ([`Naming`]) and
//! the closure that builds a column from the plan's own column type.
//!
//! Every workbook the loop creates is recorded by `create_import_book`
//! (`AppState::take_created_books`), so no caller has to count the children of
//! a folder to find them again.

use stokesforge_commands::Command;
use stokesforge_core::{Column, NodeId};

use super::AppState;
use crate::tr;

/// How an import names the books and sheets it creates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Naming {
    /// Classic import: a taken sheet name becomes "Title (2)", a refused
    /// workbook name is reported by the command.
    Classic,
    /// Origin import: the Origin name is used when the model accepts it,
    /// otherwise the automatic name stays, silently.
    Silent,
}

/// Where the sheets of one [`BookApply`] go.
pub(super) enum BookTarget {
    /// An existing workbook.
    Existing(NodeId),
    /// A new workbook, named `name` when given. If it cannot be created, one
    /// "cannot create workbook" failure is said per entry of `failed_names`.
    New {
        name: Option<String>,
        failed_names: Vec<String>,
    },
}

/// One sheet to create, with the plan columns it receives.
pub(super) struct SheetApply<C> {
    pub name: String,
    pub columns: Vec<C>,
    /// Marks the sheet "Raw Data" (classic import only, never Origin).
    pub raw: bool,
}

/// One workbook and its sheets.
pub(super) struct BookApply<C> {
    pub target: BookTarget,
    pub sheets: Vec<SheetApply<C>>,
}

/// What [`AppState::apply_book_plans`] did for one [`BookApply`].
pub(super) struct BookOutcome {
    /// The workbook, `None` when it could not be created.
    pub book: Option<NodeId>,
    /// One flag per planned sheet: was it created?
    pub sheets: Vec<bool>,
    /// For each planned sheet, one entry per planned column, in plan order: the
    /// column created, `None` when the sheet was not created, the column was
    /// skipped by `build` or the command was refused. Each id is read from the
    /// sheet's children around its own command, so a refused column never shifts
    /// the others (no positional zip, no lookup by name).
    pub columns: Vec<Vec<Option<NodeId>>>,
}

impl AppState {
    /// Creates what `books` describe, inside `parent` for the new workbooks.
    /// The caller owns the history macro. `build` turns a plan column into a
    /// model column (`None` skips it).
    pub(super) fn apply_book_plans<C>(
        &mut self,
        parent: NodeId,
        naming: Naming,
        books: Vec<BookApply<C>>,
        failures: &mut Vec<String>,
        build: &mut dyn FnMut(&C) -> Option<Column>,
    ) -> Vec<BookOutcome> {
        let mut outcomes = Vec::with_capacity(books.len());
        for book_plan in books {
            let book = match book_plan.target {
                BookTarget::Existing(book) => Some(book),
                BookTarget::New { name, failed_names } => {
                    let book = match (naming, &name) {
                        (Naming::Classic, Some(stem)) => {
                            self.create_import_book(parent, Some(stem))
                        }
                        _ => self.create_import_book(parent, None),
                    };
                    match (book, naming, &name) {
                        (Some(book), Naming::Silent, Some(name)) => {
                            self.rename_if_usable(book, name);
                        }
                        (None, ..) => {
                            for name in failed_names {
                                failures.push(tr!("error-import-create-book", name = name));
                            }
                        }
                        _ => {}
                    }
                    book
                }
            };
            let mut created = vec![false; book_plan.sheets.len()];
            let mut column_ids: Vec<Vec<Option<NodeId>>> = book_plan
                .sheets
                .iter()
                .map(|sheet| vec![None; sheet.columns.len()])
                .collect();
            if let Some(book) = book {
                for (index, sheet_plan) in book_plan.sheets.iter().enumerate() {
                    let Some(sheet) = self.add_planned_sheet(book, naming, sheet_plan, failures)
                    else {
                        continue;
                    };
                    created[index] = true;
                    for (rank, planned) in sheet_plan.columns.iter().enumerate() {
                        if let Some(column) = build(planned) {
                            let before = self.children_of(sheet);
                            self.execute(Command::AddColumn {
                                sheet,
                                index: None,
                                name: None,
                                column,
                            });
                            column_ids[index][rank] = self
                                .children_of(sheet)
                                .into_iter()
                                .find(|child| !before.contains(child));
                        }
                    }
                    if sheet_plan.raw {
                        self.execute(Command::SetSheetRawMark {
                            sheet,
                            mark: Some(stokesforge_core::RawSheetMark {
                                modified_base: tr!("import-sheet-modified-base"),
                            }),
                        });
                    }
                }
            }
            outcomes.push(BookOutcome {
                book,
                sheets: created,
                columns: column_ids,
            });
        }
        outcomes
    }

    /// The children of `id` in the current snapshot (empty when it is gone).
    fn children_of(&self, id: NodeId) -> Vec<NodeId> {
        self.snapshot
            .tree
            .node(id)
            .map(|node| node.children().to_vec())
            .unwrap_or_default()
    }

    /// Adds the empty sheet of `plan` to `book` and names it.
    fn add_planned_sheet<C>(
        &mut self,
        book: NodeId,
        naming: Naming,
        plan: &SheetApply<C>,
        failures: &mut Vec<String>,
    ) -> Option<NodeId> {
        match naming {
            Naming::Classic => self.add_named_sheet(book, &plan.name, failures),
            Naming::Silent => {
                let Some(sheet) = self.add_bare_sheet(book) else {
                    failures.push(tr!("error-import-create-sheet", name = plan.name.clone()));
                    return None;
                };
                self.rename_if_usable(sheet, &plan.name);
                Some(sheet)
            }
        }
    }

    /// The workbooks the imports created since the last call, in creation
    /// order, wherever they sit in the tree.
    pub fn take_created_books(&mut self) -> Vec<NodeId> {
        std::mem::take(&mut self.created_books)
    }
}

#[cfg(feature = "test-support")]
impl AppState {
    /// Test seam (#537): the shared apply path, in one "Import" history entry,
    /// creating one empty workbook per name inside `parent`. A later lot will
    /// reach the same path from a chosen folder.
    pub fn import_empty_books_for_test(&mut self, parent: NodeId, names: &[&str]) {
        let begun = self
            .handle
            .begin_macro(stokesforge_commands::MacroKind::Import);
        assert!(begun.is_ok(), "begin the import macro");
        let books: Vec<BookApply<()>> = names
            .iter()
            .map(|name| BookApply {
                target: BookTarget::New {
                    name: Some((*name).to_string()),
                    failed_names: vec![(*name).to_string()],
                },
                sheets: Vec::new(),
            })
            .collect();
        let mut failures = Vec::new();
        self.apply_book_plans(parent, Naming::Classic, books, &mut failures, &mut |_| None);
        assert!(failures.is_empty(), "{failures:?}");
        assert!(self.handle.end_macro().is_ok(), "end the import macro");
        self.refresh();
    }

    /// Test seam (#556): one new workbook with one sheet whose columns come
    /// from `columns` (`Some(value)` builds a column of that single value,
    /// `None` is skipped by the builder), in one "Import" history entry.
    /// Returns the ids `apply_book_plans` reports, in plan order.
    pub fn import_sheet_columns_for_test(
        &mut self,
        parent: NodeId,
        columns: Vec<Option<f64>>,
    ) -> Vec<Option<NodeId>> {
        let begun = self
            .handle
            .begin_macro(stokesforge_commands::MacroKind::Import);
        assert!(begun.is_ok(), "begin the import macro");
        let books = vec![BookApply {
            target: BookTarget::New {
                name: None,
                failed_names: vec!["Book".to_string()],
            },
            sheets: vec![SheetApply {
                name: "Data".to_string(),
                columns,
                raw: false,
            }],
        }];
        let mut failures = Vec::new();
        let mut outcomes = self.apply_book_plans(
            parent,
            Naming::Silent,
            books,
            &mut failures,
            &mut |value: &Option<f64>| value.map(|v| Column::from_f64(&[v])),
        );
        assert!(failures.is_empty(), "{failures:?}");
        assert!(self.handle.end_macro().is_ok(), "end the import macro");
        self.refresh();
        outcomes.remove(0).columns.remove(0)
    }
}

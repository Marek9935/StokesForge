//! Reading other sheets and workbooks from a formula (#403): the tree side of
//! [`crate::formula::SheetDirectory`], the inputs of a formula with their
//! membership in the sheet they must be in, and the loop check.
//!
//! # Lookup
//!
//! `Sheet3!B` looks `Sheet3` up in the workbook of the formula's sheet;
//! `[Book2]Sheet5!B` looks `Book2` up in the **folder of the formula's
//! workbook first**, then in the whole project. Several candidates outside that
//! folder are refused ([`FormulaError::AmbiguousWorkbook`]): the user writes the
//! path from the project root, `[Mesures/Book2]Sheet5!B` (`[/Book2]` for a
//! workbook at the root). Case is ignored everywhere
//! ([`crate::name::same_name_ignoring_case`], the comparison of the sibling
//! rule): two siblings that differ only by case (an older file) are refused as
//! ambiguous ([`FormulaError::AmbiguousName`]).
//!
//! # Inputs and membership
//!
//! A formula column reads the columns it names, each in a **sheet it must be
//! in**: its own, or the remote sheet of the side table. A column that is no
//! longer in that sheet (deleted, moved, sheet gone) is not **present**: the
//! formula shows `#REF` and computes to empty cells. [`Tree::formula_inputs`]
//! is the one function that says it, read by the recalculation, the loop
//! check, the messages and the column use.

use std::collections::{BTreeMap, HashSet};

use crate::compose::paired_x_columns;
use crate::formula::{
    ColumnFormula, Formula, FormulaError, SheetAddress, SheetDirectory, SheetScope,
};
use crate::id::NodeId;
use crate::name::same_name_ignoring_case;
use crate::node::NodeKind;
use crate::tree::Tree;

/// A column read by a formula, and where it must be.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FormulaInput {
    pub column: NodeId,
    /// The sheet the column must be in: the formula's own, or the remote one.
    pub sheet: NodeId,
    /// True if the column is in that sheet right now.
    pub present: bool,
}

/// The inputs of the formula of a column (#403).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FormulaInputs {
    /// The columns the formula names (explicit X column included).
    pub named: Vec<FormulaInput>,
    /// The X paired with each present column whose X the formula reads (#402).
    pub paired_x: BTreeMap<NodeId, NodeId>,
}

impl FormulaInputs {
    /// The columns this formula depends on: the present ones it names, and the
    /// X paired with them. The edges of the dependency graph.
    pub fn dependencies(&self) -> impl Iterator<Item = NodeId> + '_ {
        self.named
            .iter()
            .filter(|input| input.present)
            .map(|input| input.column)
            .chain(self.paired_x.values().copied())
    }

    /// True if every column named is where it must be.
    pub fn all_present(&self) -> bool {
        self.named.iter().all(|input| input.present)
    }
}

fn is_workbook(tree: &Tree, id: NodeId) -> bool {
    tree.node(id)
        .is_some_and(|node| matches!(node.kind(), NodeKind::Workbook { .. }))
}

impl Tree {
    /// The directory scope of the formulas of the sheet of `column`; `None` if
    /// `column` has no parent.
    pub(crate) fn scope_of(&self, column: NodeId) -> Option<SheetScope<'_>> {
        let own_sheet = self.parent(column).ok().flatten()?;
        Some(SheetScope {
            own_sheet,
            directory: self,
        })
    }

    /// The children of `parent` whose name is `name` up to case and that satisfy `keep`.
    fn named_children(
        &self,
        parent: NodeId,
        name: &str,
        keep: impl Fn(&Tree, NodeId) -> bool,
    ) -> Vec<NodeId> {
        self.children(parent)
            .unwrap_or_default()
            .iter()
            .copied()
            .filter(|child| keep(self, *child))
            .filter(|child| {
                self.node(*child)
                    .is_some_and(|node| same_name_ignoring_case(node.name(), name))
            })
            .collect()
    }

    /// The single element of `found`, or the matching refusal.
    fn only(found: Vec<NodeId>, missing: FormulaError, name: &str) -> Result<NodeId, FormulaError> {
        match found.as_slice() {
            [one] => Ok(*one),
            [] => Err(missing),
            _ => Err(FormulaError::AmbiguousName(name.to_string())),
        }
    }

    /// The workbook `spec` (the text between brackets) designates from the
    /// formula of `own_sheet`: a path from the project root if it holds a `/`,
    /// else a name looked up in the folder of the own workbook, then in the
    /// whole project.
    fn find_workbook(&self, own_sheet: NodeId, spec: &str) -> Result<NodeId, FormulaError> {
        let unknown = || FormulaError::UnknownWorkbook(spec.to_string());
        if spec.contains('/') {
            let mut segments: Vec<&str> = spec.trim_start_matches('/').split('/').collect();
            let Some(book) = segments.pop() else {
                return Err(unknown());
            };
            let mut folder = self.root();
            for segment in segments {
                let found = self.named_children(folder, segment, |tree, id| {
                    tree.node(id).is_some_and(|node| node.kind().is_folder())
                });
                folder = Self::only(found, unknown(), segment)?;
            }
            let found = self.named_children(folder, book, is_workbook);
            return Self::only(found, unknown(), book);
        }
        let own_folder = self
            .parent(own_sheet)
            .ok()
            .flatten()
            .and_then(|book| self.parent(book).ok().flatten());
        if let Some(folder) = own_folder {
            let found = self.named_children(folder, spec, is_workbook);
            if !found.is_empty() {
                return Self::only(found, unknown(), spec);
            }
        }
        let anywhere: Vec<NodeId> = self
            .iter()
            .filter(|(id, node)| {
                is_workbook(self, *id) && same_name_ignoring_case(node.name(), spec)
            })
            .map(|(id, _)| id)
            .collect();
        match anywhere.as_slice() {
            [one] => Ok(*one),
            [] => Err(unknown()),
            _ => Err(FormulaError::AmbiguousWorkbook(spec.to_string())),
        }
    }

    /// Reads a formula **typed in letters** for `column`: each letter
    /// becomes the identifier of the column it designates in the sheet of
    /// `column`, and a prefix (`Sheet3!B`, `[Book2]Sheet5!B`) the column of
    /// another sheet (see [`ColumnFormula::from_letters_in`] for the
    /// refusals). A loop is **not** checked here: [`Tree::formula_loop`] is.
    ///
    /// This is, with [`Tree::formula_letters`], the **only** letters ↔
    /// identifiers pass — shared by the application and by `pyst`. It is done
    /// *above* the commands: `Command::SetColumnMeta` never validates
    /// the references, and the gesture that changes another field keeps the
    /// stored formula as is (otherwise a `#REF` would block any
    /// change to the unit).
    pub fn formula_from_letters(
        &self,
        column: NodeId,
        text: &str,
    ) -> Result<ColumnFormula, FormulaError> {
        ColumnFormula::from_letters_in(
            text,
            self.sheet_columns_of(column),
            column,
            self.scope_of(column),
        )
    }

    /// True if the **stored** formula of `column` is part of a loop (#403): a
    /// file can hold one that input would have refused. Its cells stay empty
    /// ([`FormulaError::Cycle`] in the recalculation).
    pub fn formula_in_loop(&self, column: NodeId) -> bool {
        self.node(column)
            .and_then(crate::node::Node::as_column)
            .and_then(|node| node.meta().formula.as_ref())
            .is_some_and(|formula| self.formula_loop(column, formula))
    }

    /// [`Tree::formula_from_letters`] for a formula **being entered** (#403):
    /// a text that would make `column` read itself, in any sheet of the
    /// project, is refused with [`FormulaError::Cycle`]. The one entry point of
    /// the interface and of `pyst`; the loop check is not part of the plain
    /// reading, which tests use to build the loops of an older file on purpose.
    pub fn formula_for_entry(
        &self,
        column: NodeId,
        text: &str,
    ) -> Result<ColumnFormula, FormulaError> {
        let formula = self.formula_from_letters(column, text)?;
        if self.formula_loop(column, &formula) {
            return Err(FormulaError::Cycle);
        }
        Ok(formula)
    }

    /// Text to show for the formula of `column`: the **current** letters
    /// of its read columns, with the prefix of those of another sheet, `#REF`
    /// for a column that is no longer where it must be. `None` if `column` is
    /// not a column or has no formula. Display only: never read back to compute.
    pub fn formula_letters(&self, column: NodeId) -> Option<String> {
        let formula = self.node(column)?.as_column()?.meta().formula.as_ref()?;
        Some(formula.letters_in(self.sheet_columns_of(column), self.scope_of(column)))
    }

    /// True if the formula of `column` reads a column that is no longer where
    /// it must be (deleted, moved elsewhere, sheet gone): it is shown with
    /// [`REF_TOKEN`](crate::REF_TOKEN) and computes to
    /// [`FormulaError::DeletedColumn`].
    pub fn formula_has_deleted_reference(&self, column: NodeId) -> bool {
        self.node(column)
            .and_then(crate::node::Node::as_column)
            .and_then(|node| node.meta().formula.as_ref())
            .is_some_and(|formula| {
                formula
                    .has_deleted_reference_in(self.sheet_columns_of(column), self.scope_of(column))
            })
    }

    /// The columns the formula of `column` reads, each with the sheet it must
    /// be in and whether it is there, and the X paired with them (#403). The
    /// one answer to "what does this formula read": the recalculation (order,
    /// fingerprint, evaluation), the loop check, the messages and the column
    /// use all read it.
    pub fn formula_inputs(
        &self,
        column: NodeId,
        formula: &ColumnFormula,
        compiled: &Formula,
    ) -> FormulaInputs {
        let own_sheet = self.parent(column).ok().flatten();
        let remote = formula.remote_sheets();
        let named: Vec<FormulaInput> = compiled
            .referenced_columns()
            .into_iter()
            .filter_map(|id| {
                let sheet = remote.get(&id).copied().or(own_sheet)?;
                let present = self.parent(id).ok().flatten() == Some(sheet)
                    && self.node(id).is_some_and(|node| node.as_column().is_some());
                Some(FormulaInput {
                    column: id,
                    sheet,
                    present,
                })
            })
            .collect();
        let mut paired_x = paired_x_columns(self, compiled);
        paired_x.retain(|y, _| {
            named
                .iter()
                .any(|input| input.column == *y && input.present)
        });
        FormulaInputs { named, paired_x }
    }

    /// True if giving `candidate` as the formula of `column` would make it
    /// read itself, directly or through other formulas, in any sheet of the
    /// project (#403): a loop is refused at input. The columns it names and
    /// the X paired with them count as edges, like in the recalculation.
    /// `column` itself is judged with `candidate`, whatever it holds now. A loop
    /// that does not go through `column` (an older file's) is not reported.
    pub fn formula_loop(&self, column: NodeId, candidate: &ColumnFormula) -> bool {
        let dependencies_of = |id: NodeId, formula: &ColumnFormula| -> Vec<NodeId> {
            let Ok(compiled) = formula.compile() else {
                return Vec::new();
            };
            self.formula_inputs(id, formula, &compiled)
                .dependencies()
                .collect()
        };
        let mut stack = dependencies_of(column, candidate);
        let mut seen = HashSet::new();
        while let Some(id) = stack.pop() {
            if id == column {
                return true;
            }
            if !seen.insert(id) {
                continue;
            }
            if let Some(formula) = self
                .node(id)
                .and_then(crate::node::Node::as_column)
                .and_then(|node| node.meta().formula.as_ref())
            {
                stack.extend(dependencies_of(id, formula));
            }
        }
        false
    }
}

impl SheetDirectory for Tree {
    fn find_sheet(
        &self,
        own_sheet: NodeId,
        workbook: Option<&str>,
        sheet: &str,
    ) -> Result<NodeId, FormulaError> {
        let book = match workbook {
            Some(spec) => self.find_workbook(own_sheet, spec)?,
            None => self
                .parent(own_sheet)
                .ok()
                .flatten()
                .ok_or_else(|| FormulaError::UnknownSheet(sheet.to_string()))?,
        };
        let found = self.named_children(book, sheet, |tree, id| {
            tree.node(id).is_some_and(|node| node.kind().is_worksheet())
        });
        Self::only(found, FormulaError::UnknownSheet(sheet.to_string()), sheet)
    }

    fn columns_of(&self, sheet: NodeId) -> Option<&[NodeId]> {
        let node = self.node(sheet)?;
        node.kind().is_worksheet().then(|| node.children())
    }

    fn address_of(&self, own_sheet: NodeId, target: NodeId) -> Option<SheetAddress> {
        let node = self.node(target)?;
        if !node.kind().is_worksheet() {
            return None;
        }
        let book = node.parent()?;
        let own_book = self.parent(own_sheet).ok().flatten();
        let workbook = if own_book == Some(book) {
            None
        } else {
            let book_node = self.node(book)?;
            let short = book_node.name();
            if self.find_workbook(own_sheet, short) == Ok(book) {
                Some(short.to_string())
            } else {
                let mut path = self.folder_names_above(book).ok()?;
                // A workbook at the root needs the leading `/`, or the path
                // would read as a plain name.
                let rooted = path.is_empty();
                path.push(short.to_string());
                let joined = path.join("/");
                Some(if rooted { format!("/{joined}") } else { joined })
            }
        };
        Some(SheetAddress {
            workbook,
            sheet: node.name().to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;
    use crate::column::{Column, ColumnMeta};
    use crate::error::CoreError;
    use crate::node::{Node, WindowMeta};
    use crate::project::Project;

    /// Book1 holds Sheet1 (A, B, C) and Sheet3 (A, B, C, D); Book2 holds
    /// Sheet5 (A, B).
    struct Fx {
        project: Project,
        book1: NodeId,
        book2: NodeId,
        sheet1: NodeId,
        sheet3: NodeId,
        sheet5: NodeId,
        s1: Vec<NodeId>,
        s3: Vec<NodeId>,
    }

    fn columns(project: &mut Project, sheet: NodeId, count: usize) -> Vec<NodeId> {
        (0..count)
            .map(|k| {
                project
                    .add_column(sheet, None, None, Column::from_f64(&[k as f64 + 1.0]))
                    .expect("column")
            })
            .collect()
    }

    impl Fx {
        fn new() -> Self {
            let mut project = Project::new("Demo").expect("project");
            let root = project.root();
            let book1 = project.add_empty_workbook(root, Some("Book1")).unwrap();
            let sheet1 = project.add_worksheet(book1, Some("Sheet1")).unwrap();
            let sheet3 = project.add_worksheet(book1, Some("Sheet3")).unwrap();
            let book2 = project.add_empty_workbook(root, Some("Book2")).unwrap();
            let sheet5 = project.add_worksheet(book2, Some("Sheet5")).unwrap();
            let s1 = columns(&mut project, sheet1, 3);
            let s3 = columns(&mut project, sheet3, 4);
            columns(&mut project, sheet5, 2);
            Self {
                project,
                book1,
                book2,
                sheet1,
                sheet3,
                sheet5,
                s1,
                s3,
            }
        }

        fn tree(&self) -> &Tree {
            self.project.tree()
        }

        /// Reads `text` for the column `at` and stores it.
        fn set(&mut self, at: NodeId, text: &str) -> Result<(), FormulaError> {
            let formula = self.tree().formula_from_letters(at, text)?;
            let mut meta: ColumnMeta = self
                .tree()
                .node(at)
                .and_then(Node::as_column)
                .unwrap()
                .meta()
                .clone();
            meta.formula = Some(formula);
            self.project.set_column_meta(at, meta).unwrap();
            Ok(())
        }

        fn shown(&self, at: NodeId) -> String {
            self.tree().formula_letters(at).unwrap()
        }

        fn rename(&mut self, id: NodeId, name: &str) {
            self.project.rename(id, name).unwrap();
        }
    }

    #[test]
    fn a_prefixed_reference_reads_the_other_sheet() {
        let mut fx = Fx::new();
        let at = fx.s1[2];
        fx.set(at, "at(Sheet3!B, 340)").unwrap();
        let formula = fx
            .tree()
            .node(at)
            .unwrap()
            .as_column()
            .unwrap()
            .meta()
            .formula
            .clone()
            .unwrap();
        // Sheet3's B, not the B of the sheet that holds the formula.
        assert_eq!(formula.columns(), BTreeSet::from([fx.s3[1]]));
        assert_eq!(
            formula.remote_sheets(),
            BTreeMap::from([(fx.s3[1], fx.sheet3)])
        );
        assert_eq!(fx.shown(at), "at(Sheet3!B, 340)");
        // A column of the own sheet beside it stays local.
        fx.set(at, "B / max(Sheet3!B)").unwrap();
        let formula = fx
            .tree()
            .node(at)
            .unwrap()
            .as_column()
            .unwrap()
            .meta()
            .formula
            .clone()
            .unwrap();
        assert_eq!(formula.columns(), BTreeSet::from([fx.s1[1], fx.s3[1]]));
        assert_eq!(fx.shown(at), "B / max(Sheet3!B)");
    }

    #[test]
    fn case_is_ignored_and_the_display_restores_the_name() {
        let mut fx = Fx::new();
        let at = fx.s1[2];
        fx.set(at, "max(sheet3!b)").unwrap();
        assert_eq!(fx.shown(at), "max(Sheet3!B)");
        fx.set(at, "max([BOOK2]SHEET5!a)").unwrap();
        assert_eq!(fx.shown(at), "max([Book2]Sheet5!A)");
    }

    #[test]
    fn a_name_that_is_not_letters_and_digits_is_quoted() {
        let mut fx = Fx::new();
        let at = fx.s1[2];
        fx.set(at, "max(Sheet3!B)").unwrap();
        for (name, shown) in [
            ("Blanc 2", "max('Blanc 2'!B)"),
            ("it's", "max('it''s'!B)"),
            ("my_sheet", "max('my_sheet'!B)"),
            ("Sheet1ᵀ", "max('Sheet1ᵀ'!B)"),
            ("2nd", "max('2nd'!B)"),
            ("Été", "max('Été'!B)"),
            ("Plain9", "max(Plain9!B)"),
        ] {
            fx.rename(fx.sheet3, name);
            assert_eq!(fx.shown(at), shown, "{name}");
            // What is displayed reads back to the same column.
            let text = fx.shown(at);
            let again = fx.tree().formula_from_letters(at, &text).unwrap();
            assert_eq!(again.columns(), BTreeSet::from([fx.s3[1]]), "{name}");
        }
    }

    #[test]
    fn moving_the_sheet_to_another_workbook_changes_the_prefix_not_the_values() {
        let mut fx = Fx::new();
        let at = fx.s1[2];
        fx.set(at, "B / at(Sheet3!B, 340)").unwrap();
        let before = fx
            .tree()
            .node(at)
            .unwrap()
            .as_column()
            .unwrap()
            .meta()
            .formula
            .clone();
        fx.project.move_node(fx.sheet3, fx.book2, 0).unwrap();
        assert_eq!(fx.shown(at), "B / at([Book2]Sheet3!B, 340)");
        assert!(!fx.tree().formula_has_deleted_reference(at));
        let after = fx
            .tree()
            .node(at)
            .unwrap()
            .as_column()
            .unwrap()
            .meta()
            .formula
            .clone();
        assert_eq!(before, after, "the stored formula does not change");
    }

    #[test]
    fn a_workbook_is_looked_up_in_the_own_folder_then_in_the_project() {
        let mut fx = Fx::new();
        let root = fx.project.root();
        let folder = fx.project.add_folder(root, "Mesures").unwrap();
        let other = fx
            .project
            .add_empty_workbook(folder, Some("Book2"))
            .unwrap();
        let other_sheet = fx.project.add_worksheet(other, Some("SheetM")).unwrap();
        columns(&mut fx.project, other_sheet, 1);
        let at = fx.s1[2];
        // Book1 is at the root, where a Book2 is: it wins over the one in Mesures.
        fx.set(at, "max([Book2]Sheet5!A)").unwrap();
        assert_eq!(fx.shown(at), "max([Book2]Sheet5!A)");
        // The one in Mesures is reached by its path, and shown with it.
        fx.set(at, "max([Mesures/Book2]SheetM!A)").unwrap();
        assert_eq!(fx.shown(at), "max([Mesures/Book2]SheetM!A)");
        // A workbook that only exists elsewhere is found by its short name.
        fx.rename(other, "Book3");
        assert_eq!(fx.shown(at), "max([Book3]SheetM!A)");
        fx.set(at, "max([Book3]SheetM!A)").unwrap();
    }

    #[test]
    fn several_workbooks_of_a_name_outside_the_own_folder_ask_for_the_path() {
        let mut fx = Fx::new();
        let root = fx.project.root();
        // Book1 moves into a folder; two Book2 stay outside it.
        let own = fx.project.add_folder(root, "Own").unwrap();
        fx.project.move_node(fx.book1, own, 0).unwrap();
        let mesures = fx.project.add_folder(root, "Mesures").unwrap();
        let twin = fx
            .project
            .add_empty_workbook(mesures, Some("Book2"))
            .unwrap();
        let twin_sheet = fx.project.add_worksheet(twin, Some("Sheet5")).unwrap();
        columns(&mut fx.project, twin_sheet, 1);
        let at = fx.s1[2];
        let error = fx.set(at, "max([Book2]Sheet5!A)").unwrap_err();
        assert_eq!(error, FormulaError::AmbiguousWorkbook("Book2".to_string()));
        fx.set(at, "max([Mesures/Book2]Sheet5!A)").unwrap();
        assert_eq!(fx.shown(at), "max([Mesures/Book2]Sheet5!A)");
        fx.set(at, "max([/Book2]Sheet5!A)").unwrap();
        // The root one is shown with its rooted path while the name is ambiguous.
        assert_eq!(fx.shown(at), "max([/Book2]Sheet5!A)");
        let _ = fx.sheet5;
    }

    #[test]
    fn a_workbook_without_its_sheet_is_refused() {
        let mut fx = Fx::new();
        // Book2 has a single sheet: still refused.
        for text in ["max([Book2]B)", "max([Book2])"] {
            assert_eq!(
                fx.set(fx.s1[2], text).unwrap_err(),
                FormulaError::WorkbookWithoutSheet("Book2".to_string()),
                "{text}"
            );
        }
    }

    #[test]
    fn a_remote_column_is_read_only_by_a_column_function() {
        let mut fx = Fx::new();
        let at = fx.s1[2];
        assert_eq!(
            fx.set(at, "Sheet3!B + 1").unwrap_err(),
            FormulaError::RemoteOutsideColumnFunction
        );
        assert_eq!(
            fx.set(at, "max(Sheet3!B, A)").unwrap_err(),
            FormulaError::RemoteOutsideColumnFunction
        );
        assert_eq!(
            fx.set(at, "sqrt(Sheet3!B)").unwrap_err(),
            FormulaError::RemoteOutsideColumnFunction
        );
        assert_eq!(
            fx.set(at, "at(B, 1, Sheet3!D)").unwrap_err(),
            FormulaError::RemoteXColumn
        );
        assert_eq!(
            fx.set(at, "at(Sheet3!B, 1, Sheet3!D)").unwrap_err(),
            FormulaError::RemoteXColumn
        );
        // Allowed: the forms with a range, `at` with an explicit X.
        fx.set(at, "integral(Sheet3!B, 10, 100)").unwrap();
        fx.set(at, "max([Book2]Sheet5!B, 330, 350)").unwrap();
        fx.set(at, "mean(Sheet3!B)").unwrap();
    }

    #[test]
    fn the_x_of_a_remote_at_is_read_in_the_remote_sheet() {
        let mut fx = Fx::new();
        let at = fx.s1[2];
        // `x` is the local A; `D` exists only in Sheet3.
        fx.set(at, "at(Sheet3!B, A, D)").unwrap();
        let formula = fx
            .tree()
            .node(at)
            .unwrap()
            .as_column()
            .unwrap()
            .meta()
            .formula
            .clone()
            .unwrap();
        assert_eq!(
            formula.columns(),
            BTreeSet::from([fx.s3[1], fx.s1[0], fx.s3[3]])
        );
        assert_eq!(
            formula.remote_sheets(),
            BTreeMap::from([(fx.s3[1], fx.sheet3), (fx.s3[3], fx.sheet3)])
        );
        assert_eq!(fx.shown(at), "at(Sheet3!B, A, D)");
        // Sheet3 has no column E+; Sheet1 has no D: the contexts are not swapped.
        assert!(matches!(
            fx.set(at, "at(Sheet3!B, D, A)").unwrap_err(),
            FormulaError::UnknownColumn(_)
        ));
        // A local `at` reads its D in the own sheet, which has none.
        assert!(matches!(
            fx.set(at, "at(B, A, D)").unwrap_err(),
            FormulaError::UnknownColumn(_)
        ));
    }

    #[test]
    fn a_prefix_naming_the_own_sheet_is_a_local_reference() {
        let mut fx = Fx::new();
        let at = fx.s1[2];
        fx.set(at, "max(Sheet1!B)").unwrap();
        let formula = fx
            .tree()
            .node(at)
            .unwrap()
            .as_column()
            .unwrap()
            .meta()
            .formula
            .clone()
            .unwrap();
        assert!(formula.remote_refs().is_empty());
        assert_eq!(fx.shown(at), "max(B)");
        // Its own column is still a self-reference, with or without the prefix.
        assert_eq!(
            fx.set(at, "max(Sheet1!C)").unwrap_err(),
            FormulaError::SelfReference("C".to_string())
        );
        assert_eq!(
            fx.set(at, "max(C)").unwrap_err(),
            FormulaError::SelfReference("C".to_string())
        );
    }

    #[test]
    fn unknown_sheets_workbooks_and_columns_are_refused() {
        let mut fx = Fx::new();
        let at = fx.s1[2];
        assert_eq!(
            fx.set(at, "max(Nope!B)").unwrap_err(),
            FormulaError::UnknownSheet("Nope".to_string())
        );
        assert_eq!(
            fx.set(at, "max([Nope]Sheet5!B)").unwrap_err(),
            FormulaError::UnknownWorkbook("Nope".to_string())
        );
        assert_eq!(
            fx.set(at, "max(Sheet3!Z)").unwrap_err(),
            FormulaError::UnknownColumnIn {
                sheet: "Sheet3".to_string(),
                column: "Z".to_string()
            }
        );
        // `!=` is still a comparison.
        fx.set(at, "if(A!=B, 1, 0)").unwrap();
        fx.set(at, "if(A != B, 1, 0)").unwrap();
    }

    #[test]
    fn a_deleted_sheet_or_a_moved_column_shows_ref() {
        let mut fx = Fx::new();
        let at = fx.s1[2];
        fx.set(at, "at(Sheet3!B, 340)").unwrap();
        // The column leaves the sheet it must be in.
        let empty = fx.project.add_worksheet(fx.book1, Some("Sheet6")).unwrap();
        fx.project.move_node(fx.s3[1], empty, 0).unwrap();
        assert_eq!(fx.shown(at), "at(#REF, 340)");
        assert!(fx.tree().formula_has_deleted_reference(at));
        let compiled = fx
            .tree()
            .node(at)
            .unwrap()
            .as_column()
            .unwrap()
            .meta()
            .formula
            .clone()
            .unwrap();
        let inputs = fx
            .tree()
            .formula_inputs(at, &compiled, &compiled.compile().unwrap());
        assert!(!inputs.all_present());
        // Back in place: the formula is whole again.
        fx.project.move_node(fx.s3[1], fx.sheet3, 1).unwrap();
        assert_eq!(fx.shown(at), "at(Sheet3!B, 340)");
        // The sheet is deleted.
        fx.project.remove(fx.sheet3).unwrap();
        assert_eq!(fx.shown(at), "at(#REF, 340)");
        assert!(fx.tree().formula_has_deleted_reference(at));
    }

    #[test]
    fn loops_are_found_project_wide() {
        let mut fx = Fx::new();
        // Sheet1!C reads Sheet3!B; Sheet3!B = mean(Sheet1!C) would close a loop.
        fx.set(fx.s1[2], "max(Sheet3!B)").unwrap();
        let candidate = fx
            .tree()
            .formula_from_letters(fx.s3[1], "mean(Sheet1!C)")
            .unwrap();
        assert!(fx.tree().formula_loop(fx.s3[1], &candidate));
        // Through a third sheet of another workbook.
        fx.set(fx.s3[1], "max([Book2]Sheet5!A)").unwrap();
        let sheet5_a = fx.tree().children(fx.sheet5).unwrap()[0];
        let back = fx
            .tree()
            .formula_from_letters(sheet5_a, "max([Book1]Sheet1!C)")
            .unwrap();
        assert!(fx.tree().formula_loop(sheet5_a, &back));
        // Reading a column that reads nothing is no loop.
        let fine = fx
            .tree()
            .formula_from_letters(sheet5_a, "max([Book1]Sheet3!A)")
            .unwrap();
        assert!(!fx.tree().formula_loop(sheet5_a, &fine));
        // A local loop is refused too.
        fx.set(fx.s1[0], "B*2").unwrap();
        let local = fx.tree().formula_from_letters(fx.s1[1], "A+1").unwrap();
        assert!(fx.tree().formula_loop(fx.s1[1], &local));
    }

    #[test]
    fn a_loop_through_a_paired_x_is_found() {
        let mut fx = Fx::new();
        // Sheet3: A is an X column, B its Y.
        let mut meta = fx
            .tree()
            .node(fx.s3[0])
            .unwrap()
            .as_column()
            .unwrap()
            .meta()
            .clone();
        meta.role = crate::column::ColumnRole::X;
        fx.project.set_column_meta(fx.s3[0], meta).unwrap();
        // Sheet3!A reads Sheet1!C; Sheet1!C = integral(Sheet3!B) reads B and the
        // X paired with it, Sheet3!A: a loop that names only B.
        fx.set(fx.s3[0], "max(Sheet1!C)").unwrap();
        let candidate = fx
            .tree()
            .formula_from_letters(fx.s1[2], "integral(Sheet3!B)")
            .unwrap();
        assert!(fx.tree().formula_loop(fx.s1[2], &candidate));
        // Without a range or an `at`, no X is read: no loop.
        let candidate = fx
            .tree()
            .formula_from_letters(fx.s1[2], "max(Sheet3!B)")
            .unwrap();
        assert!(!fx.tree().formula_loop(fx.s1[2], &candidate));
    }

    #[test]
    fn a_column_that_left_its_sheet_makes_no_edge() {
        let mut fx = Fx::new();
        // Sheet3!B reads Sheet1!C; Sheet1!C = max(Sheet3!B) is a loop...
        fx.set(fx.s3[1], "max(Sheet1!C)").unwrap();
        let candidate = fx
            .tree()
            .formula_from_letters(fx.s1[2], "max(Sheet3!B)")
            .unwrap();
        assert!(fx.tree().formula_loop(fx.s1[2], &candidate));
        // ...until Sheet3!B leaves Sheet3: the candidate reads nothing there.
        let empty = fx.project.add_worksheet(fx.book1, Some("Sheet6")).unwrap();
        fx.project.move_node(fx.s3[1], empty, 0).unwrap();
        assert!(!fx.tree().formula_loop(fx.s1[2], &candidate));
    }

    #[test]
    fn siblings_may_not_differ_only_by_case() {
        let mut fx = Fx::new();
        let root = fx.project.root();
        // A sheet, a workbook, a folder and a graph.
        assert!(matches!(
            fx.project.add_worksheet(fx.book1, Some("SHEET3")),
            Err(CoreError::DuplicateName(_))
        ));
        assert!(matches!(
            fx.project.add_empty_workbook(root, Some("book1")),
            Err(CoreError::DuplicateName(_))
        ));
        fx.project.add_folder(root, "Mesures").unwrap();
        assert!(matches!(
            fx.project.add_folder(root, "MESURES"),
            Err(CoreError::DuplicateName(_))
        ));
        fx.project
            .add_graph(root, Some("Fig"), Default::default())
            .unwrap();
        assert!(matches!(
            fx.project.add_graph(root, Some("fig"), Default::default()),
            Err(CoreError::DuplicateName(_))
        ));
        // A workbook takes the name of a sibling folder up to case, too.
        assert!(matches!(
            fx.project.rename(fx.book2, "mesures"),
            Err(CoreError::DuplicateName(_))
        ));
        // Renaming a node to its own name in another case is accepted.
        fx.rename(fx.sheet3, "sheet3");
        fx.rename(fx.sheet3, "Sheet3");
        // Columns are written by letter: `a` beside `A` stays allowed.
        fx.project
            .add_column(fx.sheet1, None, Some("a"), Column::from_f64(&[1.0]))
            .unwrap();
        fx.project
            .add_column(fx.sheet1, None, Some("A2"), Column::from_f64(&[1.0]))
            .unwrap();
        fx.project
            .add_column(fx.sheet1, None, Some("a2"), Column::from_f64(&[1.0]))
            .unwrap();
        // A move into a folder holding such a name is refused as well.
        let folder = fx.project.add_folder(root, "Archive").unwrap();
        fx.project
            .add_empty_workbook(folder, Some("BOOK2"))
            .unwrap();
        assert!(matches!(
            fx.project.move_node(fx.book2, folder, 0),
            Err(CoreError::DuplicateName(_))
        ));
    }

    #[test]
    fn automatic_names_skip_a_name_taken_up_to_case() {
        let mut fx = Fx::new();
        let root = fx.project.root();
        fx.project.add_empty_workbook(root, Some("BOOK3")).unwrap();
        // Three windows exist, the generator starts from the count + 1 = Book4...
        let free = fx.project.next_window_name("Book");
        assert!(!free.eq_ignore_ascii_case("book1") && !free.eq_ignore_ascii_case("book2"));
        assert!(!free.eq_ignore_ascii_case("book3"), "{free}");
        // ... and a sheet name is skipped too.
        let book = fx.project.add_empty_workbook(root, Some("Calc")).unwrap();
        fx.project.add_worksheet(book, Some("SHEET3")).unwrap();
        fx.project.add_worksheet(book, Some("x")).unwrap();
        let sheet = fx.project.add_worksheet(book, None).unwrap();
        let name = fx.tree().node(sheet).unwrap().name().to_string();
        assert_eq!(name, "Sheet4", "SHEET3 is taken up to case");
    }

    /// An older file may hold `Blanc` and `blanc`: it loads, and a formula that
    /// names them is refused as ambiguous.
    #[test]
    fn twins_from_an_older_file_load_and_are_ambiguous_to_a_formula() {
        let id = NodeId::from_raw;
        let node = |raw: u64, name: &str, parent: Option<u64>, kind: NodeKind| {
            Node::with_parent(id(raw), name, parent.map(id), kind).unwrap()
        };
        let nodes = vec![
            node(
                1,
                "Project",
                None,
                NodeKind::Folder {
                    children: vec![id(2)],
                },
            ),
            node(
                2,
                "Book1",
                Some(1),
                NodeKind::Workbook {
                    sheets: vec![id(3), id(4), id(5)],
                    meta: WindowMeta::default(),
                },
            ),
            node(
                3,
                "Blanc",
                Some(2),
                NodeKind::Worksheet {
                    columns: vec![id(6)],
                },
            ),
            node(
                4,
                "blanc",
                Some(2),
                NodeKind::Worksheet {
                    columns: vec![id(7)],
                },
            ),
            node(
                5,
                "Own",
                Some(2),
                NodeKind::Worksheet {
                    columns: vec![id(8)],
                },
            ),
            node(6, "A", Some(3), NodeKind::Column(Column::from_f64(&[1.0]))),
            node(7, "A", Some(4), NodeKind::Column(Column::from_f64(&[1.0]))),
            node(8, "A", Some(5), NodeKind::Column(Column::from_f64(&[1.0]))),
        ];
        let tree = Tree::from_nodes(id(1), nodes).expect("an older file with twins loads");
        assert_eq!(
            tree.formula_from_letters(id(8), "max(Blanc!A)")
                .unwrap_err(),
            FormulaError::AmbiguousName("Blanc".to_string())
        );
        assert_eq!(
            tree.formula_from_letters(id(8), "max(BLANC!A)")
                .unwrap_err(),
            FormulaError::AmbiguousName("BLANC".to_string())
        );
    }
}

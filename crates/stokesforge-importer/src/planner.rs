//! The neutral planner: distributes the columns of a list of [`PlanTable`]s
//! into worksheets according to a [`FilesLayout`].
//!
//! It knows nothing of files, recipes or Origin: a table is a name plus its
//! columns (each with its X designation and whether it is kept). The classic
//! text import feeds it one table per source file ([`crate::plan_import`]);
//! a later lot of #537 feeds Origin sheets into it (one Origin sheet = one
//! table), so that both imports share the same classic layouts.

use stokesforge_import::FilesLayout;

use crate::{ImportPlan, PlannedColumn};

/// One column of a [`PlanTable`], as the planner sees it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlanColumn {
    /// The column is designated X.
    pub is_x: bool,
    /// The column is imported (an "Ignore" column is not kept).
    pub kept: bool,
}

/// A table to place: its worksheet name and its columns.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanTable {
    /// Name of the worksheet this table gives (the first one's, when shared).
    pub name: String,
    /// The columns, in order.
    pub columns: Vec<PlanColumn>,
    /// `false` for a table that could not be read: it takes no part in any plan.
    pub readable: bool,
}

/// Distributes the kept columns of the readable `tables` into worksheets
/// according to `layout`: one worksheet per table, or all side by side, or all
/// sharing the first table's X. In the plan, `item` is the table's index.
pub fn plan_tables(tables: &[PlanTable], layout: FilesLayout) -> Vec<ImportPlan> {
    let kept = |table: usize| -> Vec<usize> {
        tables[table]
            .columns
            .iter()
            .enumerate()
            .filter(|(_, column)| column.kept)
            .map(|(index, _)| index)
            .collect()
    };
    let readable: Vec<usize> = (0..tables.len()).filter(|&i| tables[i].readable).collect();
    match layout {
        FilesLayout::SheetPerFile => readable
            .iter()
            .map(|&item| ImportPlan {
                name: tables[item].name.clone(),
                columns: kept(item)
                    .into_iter()
                    .map(|column| PlannedColumn { item, column })
                    .collect(),
                items: vec![item],
            })
            .collect(),
        FilesLayout::SideBySide | FilesLayout::SharedX => {
            let Some(&first) = readable.first() else {
                return Vec::new();
            };
            let mut columns = Vec::new();
            for &item in &readable {
                for column in kept(item) {
                    // Shared X: the first one's; the following ones bring none.
                    let is_x = tables[item].columns[column].is_x;
                    if layout == FilesLayout::SharedX && item != first && is_x {
                        continue;
                    }
                    columns.push(PlannedColumn { item, column });
                }
            }
            vec![ImportPlan {
                name: tables[first].name.clone(),
                columns,
                items: readable,
            }]
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table(name: &str, roles: &[(bool, bool)]) -> PlanTable {
        PlanTable {
            name: name.to_string(),
            columns: roles
                .iter()
                .map(|&(is_x, kept)| PlanColumn { is_x, kept })
                .collect(),
            readable: true,
        }
    }

    fn xy(name: &str) -> PlanTable {
        table(name, &[(true, true), (false, true)])
    }

    fn cols(plan: &ImportPlan) -> Vec<(usize, usize)> {
        plan.columns.iter().map(|c| (c.item, c.column)).collect()
    }

    #[test]
    fn shared_x_keeps_only_the_first_x() {
        let plans = plan_tables(&[xy("a"), xy("b")], FilesLayout::SharedX);
        assert_eq!(plans.len(), 1);
        assert_eq!(plans[0].name, "a");
        assert_eq!(cols(&plans[0]), [(0, 0), (0, 1), (1, 1)]);
    }

    #[test]
    fn side_by_side_keeps_every_column() {
        let plans = plan_tables(&[xy("a"), xy("b")], FilesLayout::SideBySide);
        assert_eq!(plans.len(), 1);
        assert_eq!(cols(&plans[0]), [(0, 0), (0, 1), (1, 0), (1, 1)]);
        assert_eq!(plans[0].items, [0, 1]);
    }

    #[test]
    fn one_sheet_per_table() {
        let plans = plan_tables(&[xy("a"), xy("b")], FilesLayout::SheetPerFile);
        assert_eq!(plans.len(), 2);
        assert_eq!(plans[1].name, "b");
        assert_eq!(cols(&plans[1]), [(1, 0), (1, 1)]);
        assert_eq!(plans[1].items, [1]);
    }

    #[test]
    fn an_unkept_column_is_absent() {
        let t = table("a", &[(true, true), (false, false), (false, true)]);
        for layout in [
            FilesLayout::SheetPerFile,
            FilesLayout::SideBySide,
            FilesLayout::SharedX,
        ] {
            let plans = plan_tables(std::slice::from_ref(&t), layout);
            assert_eq!(cols(&plans[0]), [(0, 0), (0, 2)], "{layout:?}");
        }
    }

    #[test]
    fn an_unreadable_table_takes_no_part() {
        let mut bad = xy("bad");
        bad.readable = false;
        let plans = plan_tables(&[bad, xy("b")], FilesLayout::SideBySide);
        assert_eq!(plans[0].name, "b");
        assert_eq!(plans[0].items, [1]);
    }
}

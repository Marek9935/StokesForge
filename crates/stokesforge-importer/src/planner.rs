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

/// Relative tolerance of the shared-X comparison.
const SAME_X_TOLERANCE: f64 = 1e-9;

/// Do two X columns hold the same values? Same length, and each pair equal
/// within a relative `1e-9`; a missing value equals a missing value.
pub fn same_x_values(a: &[f64], b: &[f64]) -> bool {
    a.len() == b.len()
        && a.iter().zip(b).all(|(&x, &y)| {
            (x.is_nan() && y.is_nan())
                || x == y
                || (x - y).abs() <= SAME_X_TOLERANCE * x.abs().max(y.abs())
        })
}

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
/// sharing the first X. In the plan, `item` is the table's index.
///
/// Shared X (#537): only the first kept X stays; any later X (of another table
/// or a second X of the same table) for which `same_x(first, later)` holds is
/// dropped, any other stays where it is, and its table is listed in the plan's
/// `own_x`. The plan's `x_map` records, for every X met, the X that holds its
/// values (itself, or the first X when it was dropped).
pub fn plan_tables(
    tables: &[PlanTable],
    layout: FilesLayout,
    same_x: &dyn Fn(PlannedColumn, PlannedColumn) -> bool,
) -> Vec<ImportPlan> {
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
                own_x: Vec::new(),
                x_map: Vec::new(),
            })
            .collect(),
        FilesLayout::SideBySide | FilesLayout::SharedX => {
            let Some(&first) = readable.first() else {
                return Vec::new();
            };
            let mut columns = Vec::new();
            let mut first_x: Option<PlannedColumn> = None;
            let mut own_x = Vec::new();
            let mut x_map = Vec::new();
            for &item in &readable {
                for column in kept(item) {
                    let planned = PlannedColumn { item, column };
                    if layout == FilesLayout::SharedX && tables[item].columns[column].is_x {
                        match first_x {
                            None => {
                                first_x = Some(planned);
                                x_map.push((planned, planned));
                            }
                            Some(kept_x) if same_x(kept_x, planned) => {
                                x_map.push((planned, kept_x));
                                continue;
                            }
                            Some(_) => {
                                x_map.push((planned, planned));
                                if !own_x.contains(&item) {
                                    own_x.push(item);
                                }
                            }
                        }
                    }
                    columns.push(planned);
                }
            }
            vec![ImportPlan {
                name: tables[first].name.clone(),
                columns,
                items: readable,
                own_x,
                x_map,
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
        let plans = plan_tables(&[xy("a"), xy("b")], FilesLayout::SharedX, &|_, _| true);
        assert_eq!(plans.len(), 1);
        assert_eq!(plans[0].name, "a");
        assert_eq!(cols(&plans[0]), [(0, 0), (0, 1), (1, 1)]);
    }

    #[test]
    fn side_by_side_keeps_every_column() {
        let plans = plan_tables(&[xy("a"), xy("b")], FilesLayout::SideBySide, &|_, _| true);
        assert_eq!(plans.len(), 1);
        assert_eq!(cols(&plans[0]), [(0, 0), (0, 1), (1, 0), (1, 1)]);
        assert_eq!(plans[0].items, [0, 1]);
    }

    #[test]
    fn one_sheet_per_table() {
        let plans = plan_tables(&[xy("a"), xy("b")], FilesLayout::SheetPerFile, &|_, _| true);
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
            let plans = plan_tables(std::slice::from_ref(&t), layout, &|_, _| true);
            assert_eq!(cols(&plans[0]), [(0, 0), (0, 2)], "{layout:?}");
        }
    }

    #[test]
    fn an_unreadable_table_takes_no_part() {
        let mut bad = xy("bad");
        bad.readable = false;
        let plans = plan_tables(&[bad, xy("b")], FilesLayout::SideBySide, &|_, _| true);
        assert_eq!(plans[0].name, "b");
        assert_eq!(plans[0].items, [1]);
    }

    /// The tables' X values: `values[item][column]`.
    fn by_values(values: Vec<Vec<Vec<f64>>>) -> impl Fn(PlannedColumn, PlannedColumn) -> bool {
        move |a, b| same_x_values(&values[a.item][a.column], &values[b.item][b.column])
    }

    #[test]
    fn an_equal_x_is_shared_and_a_different_x_stays_and_is_reported() {
        let x = |v: &[f64]| v.to_vec();
        let ys = x(&[9.0, 9.0]);
        // (X Y), (X Y), (X Y): the second X equals the first, the third differs.
        let same = by_values(vec![
            vec![x(&[1.0, 2.0]), ys.clone()],
            vec![x(&[1.0, 2.0]), ys.clone()],
            vec![x(&[1.0, 3.0]), ys.clone()],
        ]);
        let plans = plan_tables(&[xy("a"), xy("b"), xy("c")], FilesLayout::SharedX, &same);
        assert_eq!(cols(&plans[0]), [(0, 0), (0, 1), (1, 1), (2, 0), (2, 1)]);
        assert_eq!(plans[0].own_x, [2]);
        let at = |item, column| PlannedColumn { item, column };
        assert_eq!(
            plans[0].x_map,
            [
                (at(0, 0), at(0, 0)),
                (at(1, 0), at(0, 0)),
                (at(2, 0), at(2, 0))
            ],
            "dropped X to the kept X, different X to itself"
        );
    }

    #[test]
    fn only_the_shared_x_layout_maps_x_columns() {
        for layout in [FilesLayout::SheetPerFile, FilesLayout::SideBySide] {
            let plans = plan_tables(&[xy("a"), xy("b")], layout, &|_, _| true);
            assert!(plans.iter().all(|p| p.x_map.is_empty()), "{layout:?}");
        }
    }

    #[test]
    fn a_second_x_in_one_table_is_compared_with_the_first() {
        // X Y X Y in one table: equal X, then a different one.
        let t = table(
            "a",
            &[(true, true), (false, true), (true, true), (false, true)],
        );
        let equal = by_values(vec![vec![
            vec![1.0, 2.0],
            vec![5.0, 5.0],
            vec![1.0, 2.0],
            vec![6.0, 6.0],
        ]]);
        let plans = plan_tables(std::slice::from_ref(&t), FilesLayout::SharedX, &equal);
        assert_eq!(cols(&plans[0]), [(0, 0), (0, 1), (0, 3)]);
        assert!(plans[0].own_x.is_empty());
        let different = by_values(vec![vec![
            vec![1.0, 2.0],
            vec![5.0, 5.0],
            vec![1.0, 2.5],
            vec![6.0, 6.0],
        ]]);
        let plans = plan_tables(std::slice::from_ref(&t), FilesLayout::SharedX, &different);
        assert_eq!(cols(&plans[0]), [(0, 0), (0, 1), (0, 2), (0, 3)]);
        assert_eq!(plans[0].own_x, [0]);
    }

    #[test]
    fn the_first_x_is_the_first_kept_one_and_other_layouts_never_share() {
        // The first table keeps no X: the second table's X is then the first.
        let no_x = table("a", &[(true, false), (false, true)]);
        let plans = plan_tables(&[no_x, xy("b")], FilesLayout::SharedX, &|_, _| true);
        assert_eq!(cols(&plans[0]), [(0, 1), (1, 0), (1, 1)]);
        // Side by side keeps every X even when the values are equal.
        let plans = plan_tables(&[xy("a"), xy("b")], FilesLayout::SideBySide, &|_, _| true);
        assert_eq!(cols(&plans[0]), [(0, 0), (0, 1), (1, 0), (1, 1)]);
        assert!(plans[0].own_x.is_empty());
    }

    #[test]
    fn same_x_values_tolerates_a_relative_1e9_and_equates_nan() {
        assert!(same_x_values(&[1.0, f64::NAN], &[1.0, f64::NAN]));
        assert!(same_x_values(&[1.0e6], &[1.0e6 * (1.0 + 1e-10)]));
        assert!(!same_x_values(&[1.0e6], &[1.0e6 * (1.0 + 1e-8)]));
        assert!(!same_x_values(&[1.0, 2.0], &[1.0]), "length differs");
        assert!(!same_x_values(&[1.0, f64::NAN], &[1.0, 2.0]));
        assert!(same_x_values(&[], &[]));
    }
}

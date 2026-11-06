//! Linked transposed sheets (#253): creation, completion, refusals, derived
//! states (stale, modified), refresh and life of the link.

use stokesforge_core::{
    Column, ColumnMeta, ColumnRole, ColumnType, CoreError, MixedCell, NodeId, Project, ValueBlock,
    TRANSPOSE_MARK,
};

struct Fixture {
    project: Project,
    book: NodeId,
    sheet: NodeId,
    cols: Vec<NodeId>,
}

/// Three columns (t = 0, 10, 20) of three rows, unit "a.u.".
fn fixture() -> Fixture {
    let mut project = Project::new("P").unwrap();
    let book = project.add_workbook(project.root(), Some("Book")).unwrap();
    let sheet = project.tree().children(book).unwrap()[0];
    let mut cols = Vec::new();
    for (i, data) in [[1.0, 2.0, 3.0], [4.0, 5.0, 6.0], [7.0, 8.0, 9.0]]
        .iter()
        .enumerate()
    {
        let mut column = Column::from_f64(data);
        column.set_meta(ColumnMeta {
            x_prime: Some(10.0 * i as f64),
            unit: "a.u.".into(),
            ..ColumnMeta::default()
        });
        cols.push(
            project
                .add_column(sheet, None, Some(&format!("S{i}")), column)
                .unwrap(),
        );
    }
    Fixture {
        project,
        book,
        sheet,
        cols,
    }
}

fn name(p: &Project, id: NodeId) -> String {
    p.tree().get(id).unwrap().name().to_string()
}

fn values(p: &Project, id: NodeId) -> Vec<f64> {
    p.tree()
        .node(id)
        .unwrap()
        .as_column()
        .unwrap()
        .numeric_view()
        .unwrap()
        .to_vec()
}

fn children(p: &Project, id: NodeId) -> Vec<NodeId> {
    p.tree().children(id).unwrap().to_vec()
}

fn state(p: &Project, sheet: NodeId) -> (bool, bool) {
    let s = p.tree().transpose_link(sheet).unwrap().state(p.tree());
    (s.stale, s.modified)
}

#[test]
fn creates_the_sheet_after_the_source_with_x_prime_then_the_row() {
    let mut f = fixture();
    let sheet = f.project.transpose_rows(f.sheet, &[1]).unwrap();
    assert_eq!(name(&f.project, sheet), format!("Sheet1{TRANSPOSE_MARK}"));
    let books = children(&f.project, f.book);
    assert_eq!(books, vec![f.sheet, sheet], "right after the source");
    let cols = children(&f.project, sheet);
    assert_eq!(cols.len(), 2);
    assert_eq!(name(&f.project, cols[0]), "x'");
    assert_eq!(name(&f.project, cols[1]), "Row 2");
    assert_eq!(values(&f.project, cols[0]), vec![0.0, 10.0, 20.0]);
    assert_eq!(values(&f.project, cols[1]), vec![2.0, 5.0, 8.0]);
    let meta = f
        .project
        .tree()
        .node(cols[1])
        .unwrap()
        .as_column()
        .unwrap()
        .meta()
        .clone();
    assert_eq!(meta.long_name, "Row 2 of Sheet1");
    assert_eq!(meta.unit, "a.u.");
    assert_eq!(
        f.project.tree().transposed_from(f.sheet).map(|(s, _)| s),
        Some(sheet)
    );
    assert_eq!(state(&f.project, sheet), (false, false));
}

#[test]
fn a_row_asked_again_reuses_its_column_and_a_new_row_is_appended() {
    let mut f = fixture();
    let sheet = f.project.transpose_rows(f.sheet, &[0]).unwrap();
    let first = children(&f.project, sheet)[1];
    assert_eq!(f.project.transpose_rows(f.sheet, &[0]).unwrap(), sheet);
    assert_eq!(children(&f.project, sheet).len(), 2);
    assert_eq!(children(&f.project, sheet)[1], first);
    f.project.transpose_rows(f.sheet, &[2, 0]).unwrap();
    let cols = children(&f.project, sheet);
    assert_eq!(cols.len(), 3);
    assert_eq!(name(&f.project, cols[2]), "Row 3");
    assert_eq!(f.project.tree().transposes().len(), 1);
}

#[test]
fn a_short_column_gives_a_missing_cell() {
    let mut f = fixture();
    let short = f.cols[1];
    f.project.set_length(short, 1).unwrap();
    let sheet = f.project.transpose_rows(f.sheet, &[2]).unwrap();
    let row = values(&f.project, children(&f.project, sheet)[1]);
    assert_eq!(row[0], 3.0);
    assert!(row[1].is_nan());
    assert_eq!(row[2], 9.0);
}

#[test]
fn refuses_a_column_without_x_prime_and_names_its_letter() {
    let mut f = fixture();
    f.project
        .add_column(f.sheet, None, Some("Time"), Column::from_f64(&[1.0]))
        .unwrap();
    let before = f.project.tree().clone();
    let error = f.project.transpose_rows(f.sheet, &[0]).unwrap_err();
    assert_eq!(
        error,
        CoreError::TransposeNoXPrime {
            letter: "D".into(),
            name: "Time".into()
        }
    );
    assert_eq!(
        f.project.tree().as_ref(),
        before.as_ref(),
        "nothing is created"
    );
}

#[test]
fn only_y_columns_are_transposed_and_an_x_column_without_x_prime_is_skipped() {
    let mut f = fixture();
    let mut x = Column::from_f64(&[0.0, 1.0, 2.0]);
    x.set_meta(ColumnMeta {
        role: ColumnRole::X,
        ..ColumnMeta::default()
    });
    f.project
        .add_column(f.sheet, Some(0), Some("Time"), x)
        .unwrap();
    let sheet = f.project.transpose_rows(f.sheet, &[0]).unwrap();
    let cols = children(&f.project, sheet);
    assert_eq!(values(&f.project, cols[0]), vec![0.0, 10.0, 20.0]);
    assert_eq!(values(&f.project, cols[1]), vec![1.0, 4.0, 7.0]);
    assert_eq!(state(&f.project, sheet), (false, false));
}

#[test]
fn refuses_a_text_column_but_not_a_mixed_one_without_text() {
    let mut f = fixture();
    let c = f.cols[0];
    f.project.set_column_type(c, ColumnType::Mixed).unwrap();
    assert!(f.project.transpose_rows(f.sheet, &[0]).is_ok());
    f.project
        .set_values(c, 0, ValueBlock::Mixed(vec![MixedCell::Text("n/a".into())]))
        .unwrap();
    let error = f.project.transpose_rows(f.sheet, &[1]).unwrap_err();
    assert_eq!(
        error,
        CoreError::TransposeNotNumeric {
            letter: "A".into(),
            name: "S0".into()
        }
    );
}

#[test]
fn a_taken_name_is_not_reused_unless_the_sheet_is_linked() {
    let mut f = fixture();
    let squatter = f
        .project
        .add_worksheet(f.book, Some(&format!("Sheet1{TRANSPOSE_MARK}")))
        .unwrap();
    let sheet = f.project.transpose_rows(f.sheet, &[0]).unwrap();
    assert_ne!(sheet, squatter);
    assert_eq!(name(&f.project, sheet), format!("Sheet1{TRANSPOSE_MARK} 2"));
    assert_eq!(f.project.transpose_rows(f.sheet, &[1]).unwrap(), sheet);
}

#[test]
fn stale_when_a_source_value_an_x_prime_or_the_columns_change() {
    let mut f = fixture();
    let sheet = f.project.transpose_rows(f.sheet, &[0]).unwrap();
    f.project
        .set_values(f.cols[2], 0, ValueBlock::Float(vec![99.0]))
        .unwrap();
    assert_eq!(state(&f.project, sheet), (true, false));
    f.project.recalculate_transpose(sheet).unwrap();
    assert_eq!(state(&f.project, sheet), (false, false));
    assert_eq!(
        values(&f.project, children(&f.project, sheet)[1]),
        vec![1.0, 4.0, 99.0]
    );

    let mut meta = f
        .project
        .tree()
        .node(f.cols[0])
        .unwrap()
        .as_column()
        .unwrap()
        .meta()
        .clone();
    meta.x_prime = Some(-1.0);
    f.project.set_column_meta(f.cols[0], meta).unwrap();
    assert_eq!(
        state(&f.project, sheet),
        (true, false),
        "x' read with the version"
    );
    f.project.recalculate_transpose(sheet).unwrap();
    assert_eq!(
        values(&f.project, children(&f.project, sheet)[0]),
        vec![-1.0, 10.0, 20.0]
    );

    let mut extra = Column::from_f64(&[0.0]);
    extra.set_meta(ColumnMeta {
        x_prime: Some(30.0),
        ..ColumnMeta::default()
    });
    let added = f
        .project
        .add_column(f.sheet, None, Some("S3"), extra)
        .unwrap();
    assert_eq!(state(&f.project, sheet), (true, false), "column added");
    f.project.recalculate_transpose(sheet).unwrap();
    f.project.remove(added).unwrap();
    assert_eq!(state(&f.project, sheet), (true, false), "column removed");
}

#[test]
fn only_a_value_edit_of_a_linked_column_marks_the_sheet_modified() {
    let mut f = fixture();
    let sheet = f.project.transpose_rows(f.sheet, &[0]).unwrap();
    let row = children(&f.project, sheet)[1];
    f.project.rename(row, "My spectrum").unwrap();
    f.project.set_column_type(row, ColumnType::Mixed).unwrap();
    f.project.set_column_type(row, ColumnType::Numeric).unwrap();
    f.project
        .add_column(sheet, None, Some("Mine"), Column::from_f64(&[1.0]))
        .unwrap();
    assert_eq!(state(&f.project, sheet), (false, false));
    f.project
        .set_values(row, 1, ValueBlock::Float(vec![-5.0]))
        .unwrap();
    assert_eq!(state(&f.project, sheet), (false, true));
}

#[test]
fn recalculating_a_modified_sheet_erases_the_edits_and_recreates_a_deleted_column() {
    let mut f = fixture();
    let sheet = f.project.transpose_rows(f.sheet, &[0, 1]).unwrap();
    let cols = children(&f.project, sheet);
    f.project
        .set_values(cols[1], 0, ValueBlock::Float(vec![-5.0]))
        .unwrap();
    f.project.remove(cols[2]).unwrap();
    f.project.recalculate_transpose(sheet).unwrap();
    let cols = children(&f.project, sheet);
    assert_eq!(cols.len(), 3);
    assert_eq!(values(&f.project, cols[1]), vec![1.0, 4.0, 7.0]);
    assert_eq!(values(&f.project, cols[2]), vec![2.0, 5.0, 8.0]);
    assert_eq!(state(&f.project, sheet), (false, false));
}

#[test]
fn recalculating_writes_fresh_column_versions() {
    let mut f = fixture();
    let sheet = f.project.transpose_rows(f.sheet, &[0]).unwrap();
    let row = children(&f.project, sheet)[1];
    let version = |p: &Project| p.tree().node(row).unwrap().as_column().unwrap().version();
    let before = version(&f.project);
    f.project.recalculate_transpose(sheet).unwrap();
    assert_ne!(version(&f.project), before, "data write: new version");
}

#[test]
fn completing_a_stale_untouched_sheet_refreshes_it() {
    let mut f = fixture();
    let sheet = f.project.transpose_rows(f.sheet, &[0]).unwrap();
    f.project
        .set_values(f.cols[0], 1, ValueBlock::Float(vec![42.0]))
        .unwrap();
    f.project.transpose_rows(f.sheet, &[1]).unwrap();
    assert_eq!(state(&f.project, sheet), (false, false));
    assert_eq!(
        values(&f.project, children(&f.project, sheet)[2]),
        vec![42.0, 5.0, 8.0]
    );
}

#[test]
fn the_link_follows_a_rename_and_loses_nothing_when_the_source_goes() {
    let mut f = fixture();
    let sheet = f.project.transpose_rows(f.sheet, &[0]).unwrap();
    f.project.rename(f.sheet, "New").unwrap();
    assert_eq!(name(&f.project, sheet), format!("New{TRANSPOSE_MARK}"));
    // A name the user gave themselves is no longer overwritten.
    f.project.rename(sheet, "Kinetics").unwrap();
    f.project.rename(f.sheet, "Other").unwrap();
    assert_eq!(name(&f.project, sheet), "Kinetics");
    let cols = children(&f.project, sheet);
    f.project.remove(f.sheet).unwrap();
    assert!(f.project.tree().transposes().is_empty());
    assert_eq!(children(&f.project, sheet), cols, "no data lost");
}

#[test]
fn removing_the_transposed_sheet_drops_the_link_and_a_copy_is_ordinary() {
    let mut f = fixture();
    let sheet = f.project.transpose_rows(f.sheet, &[0]).unwrap();
    let copy = f.project.duplicate_worksheet(f.sheet, "Copy").unwrap();
    assert!(f.project.tree().transposed_from(copy).is_none());
    let copy_of_transposed = f.project.duplicate_worksheet(sheet, "Copy ᵀ").unwrap();
    assert!(f
        .project
        .tree()
        .transpose_link(copy_of_transposed)
        .is_none());
    f.project.remove(sheet).unwrap();
    assert!(f.project.tree().transposes().is_empty());
}

// -- #463: Recalculate automatically -------------------------------------------

fn version(p: &Project, id: NodeId) -> u64 {
    p.tree().node(id).unwrap().as_column().unwrap().version()
}

fn formula(p: &mut Project, column: NodeId, text: &str) {
    let formula = p.tree().formula_from_letters(column, text).unwrap();
    p.set_column_meta(
        column,
        ColumnMeta {
            formula: Some(formula),
            ..ColumnMeta::default()
        },
    )
    .unwrap();
}

#[test]
fn a_new_link_is_checked_and_completing_an_unchecked_one_leaves_it_unchecked() {
    let mut f = fixture();
    let sheet = f.project.transpose_rows(f.sheet, &[0]).unwrap();
    assert!(f.project.tree().transpose_link(sheet).unwrap().auto_recalc);
    f.project.set_transpose_auto(sheet, false).unwrap();
    f.project.transpose_rows(f.sheet, &[0, 1]).unwrap();
    assert!(!f.project.tree().transpose_link(sheet).unwrap().auto_recalc);
}

#[test]
fn following_rewrites_the_rows_of_a_changed_source_and_only_when_checked() {
    let mut f = fixture();
    let sheet = f.project.transpose_rows(f.sheet, &[0, 1]).unwrap();
    let cols = children(&f.project, sheet);
    f.project
        .set_values(f.cols[1], 0, ValueBlock::Float(vec![40.0]))
        .unwrap();
    assert_eq!(state(&f.project, sheet), (true, false));
    // Unchecked: nothing happens, the snapshot stays as it was.
    f.project.set_transpose_auto(sheet, false).unwrap();
    assert!(!f.project.follow_transpose(sheet).unwrap());
    assert_eq!(values(&f.project, cols[1]), vec![1.0, 4.0, 7.0]);
    // Checked: the row follows and the sheet is up to date.
    f.project.set_transpose_auto(sheet, true).unwrap();
    assert!(f.project.follow_transpose(sheet).unwrap());
    assert_eq!(values(&f.project, cols[1]), vec![1.0, 40.0, 7.0]);
    assert_eq!(values(&f.project, cols[2]), vec![2.0, 5.0, 8.0]);
    assert_eq!(state(&f.project, sheet), (false, false));
    // Up to date: a second call does nothing.
    assert!(!f.project.follow_transpose(sheet).unwrap());
}

#[test]
fn following_keeps_the_version_of_a_column_that_comes_out_the_same() {
    let mut f = fixture();
    let sheet = f.project.transpose_rows(f.sheet, &[0, 1]).unwrap();
    let cols = children(&f.project, sheet);
    // The same value typed again: the source gets a fresh version, the numbers
    // do not change. Witness: `overwrite` instead of a comparison.
    f.project
        .set_values(f.cols[0], 0, ValueBlock::Float(vec![1.0]))
        .unwrap();
    assert_eq!(state(&f.project, sheet), (true, false));
    let before: Vec<u64> = cols.iter().map(|c| version(&f.project, *c)).collect();
    assert!(
        !f.project.follow_transpose(sheet).unwrap(),
        "nothing written"
    );
    let after: Vec<u64> = cols.iter().map(|c| version(&f.project, *c)).collect();
    assert_eq!(before, after);
    assert_eq!(state(&f.project, sheet), (false, false), "re-read");
}

#[test]
fn following_never_overwrites_a_typed_value_nor_recreates_a_deleted_column() {
    let mut f = fixture();
    let sheet = f.project.transpose_rows(f.sheet, &[0, 1]).unwrap();
    let cols = children(&f.project, sheet);
    // Rule 4: a value typed in Row 1 stops the sheet from following.
    f.project
        .set_values(cols[1], 0, ValueBlock::Float(vec![-1.0]))
        .unwrap();
    f.project
        .set_values(f.cols[1], 0, ValueBlock::Float(vec![40.0]))
        .unwrap();
    assert_eq!(state(&f.project, sheet), (true, true));
    assert!(!f.project.follow_transpose(sheet).unwrap());
    assert_eq!(values(&f.project, cols[1])[0], -1.0);
    assert_eq!(
        values(&f.project, cols[2]),
        vec![2.0, 5.0, 8.0],
        "other rows wait"
    );
    // Relink brings it all back.
    f.project.recalculate_transpose(sheet).unwrap();
    assert_eq!(values(&f.project, cols[1]), vec![1.0, 40.0, 7.0]);
    // E8: a deleted Row column is not brought back by following.
    f.project.remove(cols[2]).unwrap();
    f.project
        .set_values(f.cols[2], 0, ValueBlock::Float(vec![70.0]))
        .unwrap();
    assert!(f.project.follow_transpose(sheet).unwrap());
    assert_eq!(children(&f.project, sheet).len(), 2, "x' and Row 1 only");
    assert_eq!(values(&f.project, cols[1]), vec![1.0, 40.0, 70.0]);
}

#[test]
fn a_source_that_cannot_be_followed_keeps_the_last_numbers_and_names_the_column() {
    let mut f = fixture();
    let sheet = f.project.transpose_rows(f.sheet, &[0]).unwrap();
    let cols = children(&f.project, sheet);
    let mut bare = Column::from_f64(&[0.5, 0.5, 0.5]);
    bare.set_meta(ColumnMeta::default());
    f.project
        .add_column(f.sheet, None, Some("NoXPrime"), bare)
        .unwrap();
    let error = f.project.follow_transpose(sheet).unwrap_err();
    assert!(matches!(error, CoreError::TransposeNoXPrime { ref letter, .. } if letter == "D"));
    assert_eq!(values(&f.project, cols[1]), vec![1.0, 4.0, 7.0]);
    assert_eq!(state(&f.project, sheet), (true, false), "stays out of date");
    assert!(matches!(
        stokesforge_core::unfollowable_column(f.project.tree(), f.sheet),
        Some(CoreError::TransposeNoXPrime { .. })
    ));
}

// Rules 2 and 3 and E1: what counts as "the source calculates from its sheet".

/// Sheet2 in the same book, with one column `P`.
fn second_sheet(f: &mut Fixture) -> (NodeId, NodeId) {
    let sheet2 = f.project.add_worksheet(f.book, Some("Sheet2")).unwrap();
    let p = f
        .project
        .add_column(sheet2, None, Some("P"), Column::from_f64(&[1.0, 2.0, 3.0]))
        .unwrap();
    (sheet2, p)
}

#[test]
fn a_direct_formula_of_the_source_reading_the_followed_sheet_is_refused() {
    let mut f = fixture();
    let sheet = f.project.transpose_rows(f.sheet, &[0]).unwrap();
    let cols = children(&f.project, sheet);
    let label = f
        .project
        .add_column(
            f.sheet,
            None,
            Some("Label"),
            Column::new(ColumnType::Numeric),
        )
        .unwrap();
    let text = format!("max('{}'!B)", name(&f.project, sheet));
    // Strict entry and field: refused / kept with the error.
    let entry = f.project.tree().formula_for_entry(label, &text);
    assert!(
        matches!(
            entry,
            Err(stokesforge_core::FormulaError::ReadsFollowingSheet { .. })
        ),
        "{entry:?}"
    );
    let (kept, error) = f.project.tree().formula_keeping(label, &text);
    assert!(matches!(
        error,
        Some(stokesforge_core::FormulaError::ReadsFollowingSheet { .. })
    ));
    // Stored anyway (an older file, a paste): the question sees it, whatever
    // the role of the column (case 3: a label column).
    f.project
        .set_column_meta(
            label,
            ColumnMeta {
                role: ColumnRole::Label,
                formula: Some(kept),
                ..ColumnMeta::default()
            },
        )
        .unwrap();
    let readers = f.project.tree().columns_reading_followed_sheet();
    assert_eq!(readers.get(&label), Some(&sheet));
    // Rule 3: once unchecked, rechecking is refused, the refusal names the column.
    f.project.set_transpose_auto(sheet, false).unwrap();
    assert!(f.project.tree().columns_reading_followed_sheet().is_empty());
    let refusal = f.project.set_transpose_auto(sheet, true).unwrap_err();
    assert!(
        matches!(refusal, CoreError::TransposeSourceReadsIt { ref letter, .. } if letter == "D"),
        "{refusal:?}"
    );
    assert!(!f.project.tree().transpose_link(sheet).unwrap().auto_recalc);
    // Remove the formula: the box can be checked again.
    f.project
        .set_column_meta(label, ColumnMeta::default())
        .unwrap();
    f.project.set_transpose_auto(sheet, true).unwrap();
    let _ = cols;
}

#[test]
fn a_chain_through_another_sheet_is_refused_but_another_sheet_reading_is_not() {
    let mut f = fixture();
    let sheet = f.project.transpose_rows(f.sheet, &[0]).unwrap();
    let (_sheet2, p) = second_sheet(&mut f);
    // Sheet2!P reads the followed sheet, nothing comes back: allowed.
    let tname = name(&f.project, sheet);
    let text = format!("max('{tname}'!B)");
    let allowed = f.project.tree().formula_for_entry(p, &text);
    assert!(allowed.is_ok(), "{allowed:?}");
    formula(&mut f.project, p, &text);
    assert!(f.project.tree().columns_reading_followed_sheet().is_empty());
    // A column of Sheet1 reading Sheet2!P closes the chain: refused.
    let d = f
        .project
        .add_column(f.sheet, None, Some("D"), Column::new(ColumnType::Numeric))
        .unwrap();
    let chain = f.project.tree().formula_for_entry(d, "max(Sheet2!A)");
    assert!(
        matches!(
            chain,
            Err(stokesforge_core::FormulaError::ReadsFollowingSheet { .. })
        ),
        "{chain:?}"
    );
    // And the same entry in the other order: the formula of Sheet2 refused
    // when Sheet1's column already reads it.
    f.project.set_transpose_auto(sheet, false).unwrap();
    formula(&mut f.project, d, "max(Sheet2!A)");
    f.project.set_column_meta(p, ColumnMeta::default()).unwrap();
    f.project.set_transpose_auto(sheet, true).unwrap();
    let late = f.project.tree().formula_for_entry(p, &text);
    assert!(
        matches!(
            late,
            Err(stokesforge_core::FormulaError::ReadsFollowingSheet { .. })
        ),
        "{late:?}"
    );
}

#[test]
fn an_analysis_reading_the_followed_sheet_may_not_land_in_the_source() {
    use stokesforge_analysis::correct::CorrectOp;
    use stokesforge_analysis::ops::Operation;
    use stokesforge_core::{CalculationDraft, OutputDraft, SeriesRef, SeriesResult};

    let mut f = fixture();
    let sheet = f.project.transpose_rows(f.sheet, &[0]).unwrap();
    let cols = children(&f.project, sheet);
    let draft = |x: NodeId, y: NodeId| CalculationDraft {
        operation: Operation::Correct {
            op: CorrectOp::Divide,
        },
        series: vec![SeriesRef {
            x: Some(x),
            y,
            sigma: None,
        }],
        aux: None,
        outputs: vec![OutputDraft {
            series: 0,
            key: "corrected".to_string(),
            index: None,
            meta: ColumnMeta::default(),
            data: vec![1.0, 2.0, 3.0],
        }],
        results: vec![SeriesResult::default()],
        keep_results: false,
        auto_recalc: true,
        low_r2_dialog: true,
    };
    // Y in the source, x' from the followed sheet: the result lands in the
    // source (case 4). Refused, and nothing is left behind.
    let before = f.project.tree().clone();
    let refused = f.project.add_calculation(draft(cols[0], f.cols[0]));
    assert!(
        matches!(
            refused,
            Err(CoreError::CalculationReadsFollowingSheet { .. })
        ),
        "{refused:?}"
    );
    assert_eq!(f.project.tree().as_ref(), before.as_ref());
    // Analyzing a Row of the followed sheet lands in that sheet: allowed.
    f.project.add_calculation(draft(cols[0], cols[1])).unwrap();
    // Unchecked, the source may calculate from the sheet.
    f.project.set_transpose_auto(sheet, false).unwrap();
    f.project
        .add_calculation(draft(cols[0], f.cols[0]))
        .unwrap();
}

#[test]
fn a_loop_through_two_followed_sheets_is_refused() {
    let mut f = fixture();
    let first = f.project.transpose_rows(f.sheet, &[0]).unwrap();
    let sheet2 = f.project.add_worksheet(f.book, Some("Sheet2")).unwrap();
    let mut p = Column::from_f64(&[1.0, 2.0, 3.0]);
    p.set_meta(ColumnMeta {
        x_prime: Some(5.0),
        ..ColumnMeta::default()
    });
    f.project.add_column(sheet2, None, Some("P"), p).unwrap();
    let second = f.project.transpose_rows(sheet2, &[0]).unwrap();
    // Added after the link: it is a Y column of Sheet2, so the second sheet
    // depends on it (the link is an edge of the graph).
    let q = f
        .project
        .add_column(sheet2, None, Some("Q"), Column::new(ColumnType::Numeric))
        .unwrap();
    let d = f
        .project
        .add_column(f.sheet, None, Some("D"), Column::new(ColumnType::Numeric))
        .unwrap();
    // Sheet1!D reads the second sheet: no loop yet.
    let text = format!("max('{}'!B)", name(&f.project, second));
    assert!(f.project.tree().formula_for_entry(d, &text).is_ok());
    formula(&mut f.project, d, &text);
    // Sheet2!Q reading the first sheet closes the loop D -> second sheet ->
    // Sheet2 (link) -> Q -> first sheet -> Sheet1 (link) -> D.
    let closing = format!("max('{}'!B)", name(&f.project, first));
    let refused = f.project.tree().formula_for_entry(q, &closing);
    assert!(
        matches!(
            refused,
            Err(stokesforge_core::FormulaError::ReadsFollowingSheet { .. })
        ),
        "{refused:?}"
    );
}

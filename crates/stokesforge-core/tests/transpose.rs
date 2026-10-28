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

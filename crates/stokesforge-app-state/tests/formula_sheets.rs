//! Formulas that read other sheets (#403) seen from the application state: a
//! loop is refused at typing (project-wide, local or remote) with the loop
//! message, the refusal is kept between frames but follows the project, and a
//! file that already holds a loop frames the Formula cell.

use std::collections::BTreeMap;

use stokesforge_app_state::i18n::{self, Language};
use stokesforge_app_state::state::{Action, AppState, HeaderField};
use stokesforge_core::{
    Column, ColumnFormula, ColumnMeta, FormulaPart, NodeId, Project, RemoteRef,
};
use stokesforge_session::Session;

const LOOP_MESSAGE: &str = "Formulas refer to each other in a loop. Remove one of the references";

struct Setup {
    _session: Session,
    state: AppState,
    /// `Sheet1`: A, B, C, D.
    s1: Vec<NodeId>,
    /// `Sheet3`: A, B.
    s3: Vec<NodeId>,
    sheet1: NodeId,
    sheet3: NodeId,
}

/// Two sheets; `prepare` may install formulas the way a file would hold them.
fn setup(dir: &std::path::Path, prepare: impl FnOnce(&mut Project, &[NodeId], &[NodeId])) -> Setup {
    i18n::set_language(Language::English);
    let mut project = Project::new("P").unwrap();
    let book = project.add_workbook(project.root(), Some("Book")).unwrap();
    let sheet1 = project.tree().children(book).unwrap()[0];
    let sheet3 = project.add_worksheet(book, Some("Sheet3")).unwrap();
    let values = [1.0, 2.0, 3.0];
    let mut columns = |sheet: NodeId, names: &[&str]| -> Vec<NodeId> {
        names
            .iter()
            .map(|name| {
                project
                    .add_column(sheet, None, Some(name), Column::from_f64(&values))
                    .unwrap()
            })
            .collect()
    };
    let s1 = columns(sheet1, &["A", "B", "C", "D"]);
    let s3 = columns(sheet3, &["A", "B"]);
    prepare(&mut project, &s1, &s3);
    let session = Session::spawn(project, None).unwrap();
    let state = AppState::new_with_preferences(
        session.handle(),
        dir.join("p.stf").display().to_string(),
        stokesforge_app_state::preferences::Preferences::load_from(None),
    )
    .unwrap();
    Setup {
        _session: session,
        state,
        s1,
        s3,
        sheet1,
        sheet3,
    }
}

fn type_formula(s: &mut Setup, column: NodeId, text: &str) {
    s.state.begin_header_edit(column, HeaderField::Formula);
    s.state.header_editing_mut().unwrap().buffer = text.to_string();
}

fn commit(s: &mut Setup) {
    s.state.commit_header_edit();
}

fn formula_text(s: &Setup, column: NodeId) -> Option<String> {
    s.state.tree().formula_letters(column)
}

#[test]
fn a_loop_across_sheets_is_refused_at_typing() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = setup(dir.path(), |_, _, _| {});
    let (c, a3) = (s.s1[2], s.s3[0]);
    type_formula(&mut s, c, "max(Sheet3!A)");
    assert_eq!(s.state.header_edit_refusal(), None, "no loop yet");
    commit(&mut s);
    assert_eq!(formula_text(&s, c).as_deref(), Some("max(Sheet3!A)"));

    // Sheet3!A reading Sheet1!C would close the loop.
    type_formula(&mut s, a3, "max(Sheet1!C)");
    assert_eq!(s.state.header_edit_refusal().as_deref(), Some(LOOP_MESSAGE));
    commit(&mut s);
    assert_eq!(formula_text(&s, a3), None, "nothing is written");

    // A loop through a third column (A -> B -> Sheet1!C -> Sheet3!A), then a
    // purely local one.
    let (b3, d) = (s.s3[1], s.s1[3]);
    type_formula(&mut s, b3, "max(Sheet1!C)");
    assert_eq!(s.state.header_edit_refusal(), None);
    commit(&mut s);
    type_formula(&mut s, a3, "max(B)");
    assert_eq!(s.state.header_edit_refusal().as_deref(), Some(LOOP_MESSAGE));
    commit(&mut s);
    assert_eq!(formula_text(&s, a3), None);
    type_formula(&mut s, d, "A+1");
    assert_eq!(s.state.header_edit_refusal(), None);
    commit(&mut s);
    let a1 = s.s1[0];
    type_formula(&mut s, a1, "D+1");
    assert_eq!(s.state.header_edit_refusal().as_deref(), Some(LOOP_MESSAGE));
}

/// The refusal is kept for the frames that follow, but follows the project:
/// the loop disappears when the formula that closed it changes.
#[test]
fn the_kept_refusal_follows_the_project() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = setup(dir.path(), |_, _, _| {});
    let (c, a3) = (s.s1[2], s.s3[0]);
    type_formula(&mut s, c, "max(Sheet3!A)");
    commit(&mut s);
    type_formula(&mut s, a3, "max(Sheet1!C)");
    assert_eq!(s.state.header_edit_refusal().as_deref(), Some(LOOP_MESSAGE));
    assert_eq!(
        s.state.header_edit_refusal().as_deref(),
        Some(LOOP_MESSAGE),
        "same answer between frames"
    );
    // The formula of Sheet1!C changes while Sheet3!A is still being typed.
    s.state.dispatch(Action::SetColumnFormula {
        column: c,
        formula: Some("A".to_string()),
    });
    assert_eq!(
        s.state.header_edit_refusal(),
        None,
        "the project changed: asked again"
    );
}

/// A file can hold a loop that input refuses: the Formula cell is framed with
/// the loop message, and a reader of the loop is not.
#[test]
fn a_loop_held_by_a_file_frames_the_formula_cell() {
    let dir = tempfile::tempdir().unwrap();
    let mut ids = (NodeId::from_raw(0), NodeId::from_raw(0));
    let s = setup(dir.path(), |project, s1, s3| {
        let remote_read = |read: NodeId, sheet: NodeId| {
            ColumnFormula::from_parts_with_remote(
                vec![
                    FormulaPart::Text("max(".to_string()),
                    FormulaPart::Column(read),
                    FormulaPart::Text(")".to_string()),
                ],
                BTreeMap::from([(
                    1,
                    RemoteRef {
                        sheet,
                        prefixed: true,
                    },
                )]),
            )
        };
        let sheet1 = project.tree().parent(s1[0]).unwrap().unwrap();
        let sheet3 = project.tree().parent(s3[0]).unwrap().unwrap();
        for (column, read, sheet) in [(s1[2], s3[0], sheet3), (s3[0], s1[2], sheet1)] {
            project
                .set_column_meta(
                    column,
                    ColumnMeta {
                        formula: Some(remote_read(read, sheet)),
                        ..Default::default()
                    },
                )
                .unwrap();
        }
        // D reads the loop without being part of it.
        let reader = project.tree().formula_from_letters(s1[3], "C+1").unwrap();
        project
            .set_column_meta(
                s1[3],
                ColumnMeta {
                    formula: Some(reader),
                    ..Default::default()
                },
            )
            .unwrap();
        ids = (s1[2], s1[3]);
    });
    assert_eq!(s.state.formula_issue(ids.0).as_deref(), Some(LOOP_MESSAGE));
    assert_eq!(
        s.state.formula_issue(s.s3[0]).as_deref(),
        Some(LOOP_MESSAGE)
    );
    assert_eq!(s.state.formula_issue(ids.1), None);
    let _ = (s.sheet1, s.sheet3);
}

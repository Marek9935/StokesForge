//! A paste that goes beyond the last column creates the missing columns
//! (#495, replaces the #311 "dropped and counted" for columns), in the same
//! single Undo entry, with no cap.

use stokesforge_app_state::i18n::{self, Language};
use stokesforge_app_state::state::{Action, AppState, HeaderField, StatusLevel};
use stokesforge_core::{Cell, Column, NodeId, Project};
use stokesforge_session::Session;

struct Setup {
    _language: i18n::LanguageGuard,
    _session: Session,
    state: AppState,
    sheet: NodeId,
    cols: Vec<NodeId>,
}

/// A sheet with columns `A`, `B` of three rows (1, 2, 3 and 10, 20, 30).
fn setup(dir: &std::path::Path) -> Setup {
    let language = i18n::test_language(Language::English);
    let mut project = Project::new("P").unwrap();
    let book = project.add_workbook(project.root(), Some("Book")).unwrap();
    let sheet = project.tree().children(book).unwrap()[0];
    let cols: Vec<NodeId> = [("A", [1.0, 2.0, 3.0]), ("B", [10.0, 20.0, 30.0])]
        .iter()
        .map(|(name, values)| {
            project
                .add_column(sheet, None, Some(name), Column::from_f64(values))
                .unwrap()
        })
        .collect();
    let session = Session::spawn(project, None).unwrap();
    let state = AppState::new_with_preferences(
        session.handle(),
        dir.join("p.stf").display().to_string(),
        stokesforge_app_state::preferences::Preferences::load_from(None),
    )
    .unwrap();
    let mut s = Setup {
        _language: language,
        _session: session,
        state,
        sheet,
        cols,
    };
    while s.state.can_undo() {
        s.state.dispatch(Action::Undo);
    }
    s
}

impl Setup {
    fn order(&self) -> Vec<NodeId> {
        self.state
            .visible_columns(self.sheet)
            .into_iter()
            .map(|(id, _, _)| id)
            .collect()
    }

    fn cell(&self, column: NodeId, row: usize) -> Cell {
        self.state
            .tree()
            .get(column)
            .unwrap()
            .as_column()
            .unwrap()
            .get(row)
    }

    fn paste(&mut self, text: &str) {
        self.state.dispatch(Action::Paste {
            sheet: self.sheet,
            text: text.to_string(),
        });
    }

    fn status(&self) -> (StatusLevel, String) {
        let s = self.state.status().expect("a status");
        (s.level, s.text.clone())
    }

    /// One Undo removes everything the paste did, and nothing more.
    fn assert_one_undo_restores(&mut self, columns: usize) {
        self.state.dispatch(Action::Undo);
        assert_eq!(self.order().len(), columns, "created columns removed");
        assert!(!self.state.can_undo(), "a single Undo reverted the paste");
    }
}

/// Three columns pasted on the last one: two are created, the values are in
/// them, one notice, one Undo.
#[test]
fn three_columns_pasted_on_the_last_column_create_two_in_one_undo() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = setup(dir.path());
    s.state.select_cell(s.sheet, s.cols[1], 0);
    s.paste("7\t8\t9\n4\t5\t6");
    let order = s.order();
    assert_eq!(order.len(), 4);
    assert_eq!(s.cell(order[1], 0), Cell::Float(7.0));
    assert_eq!(s.cell(order[2], 0), Cell::Float(8.0));
    assert_eq!(s.cell(order[3], 1), Cell::Float(6.0));
    let (level, text) = s.status();
    assert_eq!(level, StatusLevel::Info);
    assert!(text.contains("2 columns were added"), "{text}");
    s.assert_one_undo_restores(2);
    assert_eq!(s.cell(s.cols[1], 0), Cell::Float(10.0));
}

/// Ctrl+Shift+C of columns A and B (B reads A) pasted on the last column:
/// the created column carries the formula and computes in the destination.
#[test]
fn a_formulas_copy_beyond_the_edge_creates_a_computing_column() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = setup(dir.path());
    s.state.begin_header_edit(s.cols[1], HeaderField::Formula);
    s.state.header_editing_mut().unwrap().buffer = "A*2".to_string();
    s.state.commit_header_edit();
    s.state.begin_header_edit(s.cols[1], HeaderField::Formula);
    s.state.header_editing_mut().unwrap().buffer = "A*2".to_string();
    s.state.commit_header_edit();
    let before = s.state.snapshot().history.undo_label.clone();

    s.state.select_column(s.sheet, s.cols[0]);
    s.state.toggle_column_selection(s.sheet, s.cols[1]);
    assert!(s.state.copy_full_columns(s.sheet).is_none(), "asks first");
    s.state.dispatch(Action::ConfirmFullColumnsCopy(
        stokesforge_app_state::state::FullColumnsCopy::Formulas,
    ));
    s.state.select_cell(s.sheet, s.cols[1], 0);
    let text = s.state.take_object_clipboard_text().unwrap();
    s.paste(&text);
    let order = s.order();
    assert_eq!(order.len(), 3, "one column created");
    assert!(
        s.status().1.contains("One column was added"),
        "{:?}",
        s.status()
    );
    assert_eq!(s.cell(order[2], 0), Cell::Float(2.0), "A*2 computes");
    assert_eq!(s.cell(order[2], 2), Cell::Float(6.0));
    assert_ne!(s.state.snapshot().history.undo_label, before);
    s.state.dispatch(Action::Undo);
    assert_eq!(s.order().len(), 2, "one Undo removes the created column");
}

/// Paste in Place (Ctrl+Shift+V) goes through the same creation.
#[test]
fn paste_in_place_beyond_the_edge_creates_the_columns() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = setup(dir.path());
    s.state.select_cell(s.sheet, s.cols[0], 0);
    s.state.extend_selection(s.sheet, s.cols[1], 1);
    s.state.copy_selection(s.sheet).unwrap();
    s.state.select_cell(s.sheet, s.cols[1], 0);
    s.state.dispatch(Action::PasteAbsolute { sheet: s.sheet });
    let order = s.order();
    assert_eq!(order.len(), 3, "one column created");
    assert_eq!(s.cell(order[2], 0), Cell::Float(10.0));
    s.assert_one_undo_restores(2);
}

/// An outside number no column reading numbers can hold is refused as before:
/// nothing pasted, no column created, no history entry.
#[test]
fn an_out_of_range_number_in_a_would_be_new_column_is_refused_whole() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = setup(dir.path());
    s.state.select_cell(s.sheet, s.cols[1], 0);
    s.paste("1\t1e400");
    assert_eq!(s.order().len(), 2, "no column created");
    assert_eq!(s.cell(s.cols[1], 0), Cell::Float(10.0), "nothing pasted");
    assert!(!s.state.can_undo(), "no history");
    assert!(s.state.status().unwrap().is_error());
}

/// No cap (#495): a very wide paste creates every missing column, in one Undo,
/// with a plain notice and no orange.
#[test]
fn a_very_wide_paste_creates_every_column_in_one_undo() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = setup(dir.path());
    let wanted = 1006;
    let row: Vec<String> = (0..wanted).map(|i| i.to_string()).collect();
    s.state.select_cell(s.sheet, s.cols[1], 0);
    s.paste(&row.join("\t"));
    assert_eq!(s.order().len(), 1 + wanted);
    let (level, text) = s.status();
    assert_eq!(level, StatusLevel::Info);
    assert!(text.contains("1005 columns were added"), "{text}");
    s.assert_one_undo_restores(2);
}

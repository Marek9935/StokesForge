//! Paste Transposed (#495, Ctrl+Alt+V): the copied block rotated a quarter
//! turn, then pasted like Paste: frozen values, one Undo, columns created
//! beyond the edge. The x' row transposes like data.

use stokesforge_app_state::i18n::{self, Language};
use stokesforge_app_state::state::{Action, AppState, FullColumnsCopy, HeaderField, StatusLevel};
use stokesforge_core::{Cell, Column, ColumnType, NodeId, Project};
use stokesforge_session::Session;

struct Setup {
    _language: i18n::LanguageGuard,
    _session: Session,
    state: AppState,
    sheet: NodeId,
    /// `A`, `B`, `C` (3 rows each), then `S` (2 rows) and `D` (= `A*2`) when
    /// asked.
    cols: Vec<NodeId>,
}

fn setup(dir: &std::path::Path, extras: bool) -> Setup {
    let language = i18n::test_language(Language::English);
    let mut project = Project::new("P").unwrap();
    let book = project.add_workbook(project.root(), Some("Book")).unwrap();
    let sheet = project.tree().children(book).unwrap()[0];
    let mut cols: Vec<NodeId> = Vec::new();
    let mut add = |name: &str, values: &[f64]| {
        cols.push(
            project
                .add_column(sheet, None, Some(name), Column::from_f64(values))
                .unwrap(),
        );
    };
    add("A", &[1.0, 2.0, 3.0]);
    add("B", &[10.0, 20.0, 30.0]);
    add("C", &[100.0, 200.0, 300.0]);
    if extras {
        add("S", &[7.0, 8.0]);
    }
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
    if extras {
        s.state.dispatch(Action::AddColumn { sheet });
        let last = *s.order().last().unwrap();
        s.cols.push(last);
        s.set_header(last, HeaderField::Formula, "A*2");
    } else {
        // The tests with the extra columns do not look at the history.
        while s.state.can_undo() {
            s.state.dispatch(Action::Undo);
        }
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

    fn float(&self, column: NodeId, row: usize) -> f64 {
        match self.cell(column, row) {
            Cell::Float(value) => value,
            other => panic!("not a number: {other:?}"),
        }
    }

    fn set_header(&mut self, column: NodeId, field: HeaderField, text: &str) {
        self.state.begin_header_edit(column, field);
        self.state.header_editing_mut().unwrap().buffer = text.to_string();
        self.state.commit_header_edit();
    }

    fn select_range(&mut self, from: (NodeId, usize), to: (NodeId, usize)) {
        self.state.select_cell(self.sheet, from.0, from.1);
        self.state.extend_selection(self.sheet, to.0, to.1);
    }

    fn transposed(&mut self, text: &str) {
        self.state.dispatch(Action::PasteTransposed {
            sheet: self.sheet,
            text: text.to_string(),
        });
    }

    fn status(&self) -> (StatusLevel, String) {
        let s = self.state.status().expect("a status");
        (s.level, s.text.clone())
    }

    fn undo_back_to_nothing(&mut self, columns: usize) {
        self.state.dispatch(Action::Undo);
        assert_eq!(self.order().len(), columns, "column count after Undo");
        assert!(!self.state.can_undo(), "one Undo reverted the whole paste");
    }
}

/// A column of 3 values copied with Ctrl+C becomes a row of 3: two columns
/// receive it, a third is created; one Undo.
#[test]
fn a_copied_column_pastes_as_a_row_in_one_undo() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = setup(dir.path(), false);
    s.select_range((s.cols[0], 0), (s.cols[0], 2));
    let text = s.state.copy_selection(s.sheet).unwrap();
    s.state.select_cell(s.sheet, s.cols[1], 0);
    s.transposed(&text);
    let order = s.order();
    assert_eq!(order.len(), 4, "one column created");
    assert_eq!(s.float(order[1], 0), 1.0);
    assert_eq!(s.float(order[2], 0), 2.0);
    assert_eq!(s.float(order[3], 0), 3.0);
    assert_eq!(s.float(order[1], 1), 20.0, "the other rows are untouched");
    s.undo_back_to_nothing(3);
    assert_eq!(s.float(s.cols[1], 0), 10.0);
}

/// A row copied becomes a column.
#[test]
fn a_copied_row_pastes_as_a_column() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = setup(dir.path(), false);
    s.select_range((s.cols[0], 1), (s.cols[2], 1));
    let text = s.state.copy_selection(s.sheet).unwrap();
    s.state.select_cell(s.sheet, s.cols[0], 0);
    s.transposed(&text);
    assert_eq!(s.order().len(), 3, "no column needed");
    assert_eq!(
        [0, 1, 2].map(|row| s.float(s.cols[0], row)),
        [2.0, 20.0, 200.0]
    );
    s.undo_back_to_nothing(3);
}

/// Ctrl+Shift+C of a column carrying a name, a unit and a comment: only the
/// values are pasted, the headers of the target stay.
#[test]
fn a_full_column_copy_pastes_its_values_only() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = setup(dir.path(), false);
    s.set_header(s.cols[0], HeaderField::LongName, "Name A");
    s.set_header(s.cols[0], HeaderField::Unit, "uA");
    s.set_header(s.cols[0], HeaderField::Comment, "note A");
    s.set_header(s.cols[1], HeaderField::LongName, "Name B");
    s.state.select_column(s.sheet, s.cols[0]);
    let text = s.state.copy_full_columns(s.sheet).unwrap();
    s.state.select_cell(s.sheet, s.cols[1], 0);
    s.transposed(&text);
    let order = s.order();
    assert_eq!(s.float(order[1], 0), 1.0);
    assert_eq!(s.float(order[2], 0), 2.0);
    assert_eq!(s.float(order[3], 0), 3.0);
    let meta = s.state.column_meta(s.cols[1]);
    assert_eq!(meta.long_name, "Name B");
    assert!(meta.unit.is_empty() && meta.comment.is_empty());
    assert!(s.state.column_meta(order[3]).long_name.is_empty());
}

/// A formula column copied as Values pastes its frozen values; the same
/// copy as Formulas pastes values too.
#[test]
fn a_formula_column_pastes_frozen_values() {
    for choice in [FullColumnsCopy::Values, FullColumnsCopy::Formulas] {
        let dir = tempfile::tempdir().unwrap();
        let mut s = setup(dir.path(), true);
        let formula = s.cols[4];
        s.state.select_column(s.sheet, formula);
        let _ = s.state.copy_full_columns(s.sheet);
        s.state.dispatch(Action::ConfirmFullColumnsCopy(choice));
        let text = s.state.take_object_clipboard_text().unwrap();
        s.state.select_cell(s.sheet, s.cols[0], 0);
        s.transposed(&text);
        assert_eq!(
            [0, 1, 2].map(|column| s.float(s.cols[column], 0)),
            [2.0, 4.0, 6.0]
        );
        assert!(
            s.state.column_meta(s.cols[1]).formula.is_none(),
            "no formula travels"
        );
    }
}

/// A block of 2 columns of different lengths is rotated; the cell past the
/// end of the short column is missing in the result.
#[test]
fn columns_of_different_lengths_leave_a_missing_cell() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = setup(dir.path(), true);
    s.select_range((s.cols[2], 0), (s.cols[3], 2));
    let text = s.state.copy_selection(s.sheet).unwrap();
    s.state.select_cell(s.sheet, s.cols[0], 0);
    s.transposed(&text);
    assert_eq!(
        [0, 1, 2].map(|column| s.float(s.cols[column], 0)),
        [100.0, 200.0, 300.0]
    );
    assert_eq!(s.float(s.cols[0], 1), 7.0);
    assert_eq!(s.float(s.cols[1], 1), 8.0);
    assert!(matches!(s.cell(s.cols[2], 1), Cell::Float(v) if v.is_nan()));
}

/// A column of 10 values pasted on a 3-column sheet creates 7 columns.
#[test]
fn a_long_column_creates_the_columns_it_needs() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = setup(dir.path(), false);
    let text: Vec<String> = (1..=10).map(|n| n.to_string()).collect();
    s.state.select_cell(s.sheet, s.cols[0], 0);
    s.transposed(&text.join("\n"));
    let order = s.order();
    assert_eq!(order.len(), 10);
    assert_eq!(s.float(order[9], 0), 10.0);
    assert!(
        s.status().1.contains("7 columns were added"),
        "{:?}",
        s.status()
    );
    s.undo_back_to_nothing(3);
}

/// An outside number out of range is refused whole, the message naming the
/// number and its row in the transposed block; nothing is created.
#[test]
fn an_out_of_range_outside_number_is_refused_whole() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = setup(dir.path(), false);
    s.state.select_cell(s.sheet, s.cols[0], 0);
    s.transposed("1\n2\n1e400");
    let (level, text) = s.status();
    assert_eq!(level, StatusLevel::Error);
    assert!(text.contains("1e400") && text.contains("row 1"), "{text}");
    assert_eq!(s.order().len(), 3);
    assert_eq!(s.float(s.cols[0], 0), 1.0);
    assert!(!s.state.can_undo(), "no history");
}

/// An Excel table `a b / c d` lands as `a c / b d`; text widens a Numeric
/// column to Mixed.
#[test]
fn a_spreadsheet_table_is_rotated_and_text_widens_the_column() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = setup(dir.path(), false);
    s.state.select_cell(s.sheet, s.cols[0], 0);
    s.transposed("a\tb\r\nc\td\r\n");
    let text = |column, row| match s.cell(column, row) {
        Cell::Text(Some(text)) => text,
        other => panic!("not a text: {other:?}"),
    };
    assert_eq!(text(s.cols[0], 0), "a");
    assert_eq!(text(s.cols[1], 0), "c");
    assert_eq!(text(s.cols[0], 1), "b");
    assert_eq!(text(s.cols[1], 1), "d");
    let kind = s
        .state
        .tree()
        .get(s.cols[0])
        .unwrap()
        .as_column()
        .unwrap()
        .column_type();
    assert_eq!(kind, ColumnType::Mixed);
}

/// Anchored on the entry row (#440), the paste adds the row; one Undo.
#[test]
fn the_entry_row_takes_a_transposed_paste() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = setup(dir.path(), false);
    let entry = s.state.entry_row(s.sheet);
    assert_eq!(entry, 3);
    s.state.select_cell(s.sheet, s.cols[0], entry);
    s.transposed("5\n6");
    assert_eq!(s.state.row_count(s.sheet), 4);
    assert_eq!(s.float(s.cols[0], 3), 5.0);
    assert_eq!(s.float(s.cols[1], 3), 6.0);
    s.undo_back_to_nothing(3);
    assert_eq!(s.state.row_count(s.sheet), 3);
}

/// A formula column refuses its cell, the others are pasted, the status says
/// it as an error.
#[test]
fn a_formula_target_column_refuses_and_the_others_are_pasted() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = setup(dir.path(), true);
    // Row [1..5] over A, B, C, S, D: D is the formula column.
    s.state.select_cell(s.sheet, s.cols[0], 0);
    s.transposed("7\n8\n9\n4\n5");
    assert_eq!(s.float(s.cols[0], 0), 7.0);
    assert_eq!(s.float(s.cols[2], 0), 9.0);
    assert_eq!(s.float(s.cols[3], 0), 4.0);
    assert_eq!(s.status().0, StatusLevel::Error);
}

/// On a header field other than x' the transposed block has no place: nothing
/// is pasted, no history. A single value pastes like Paste, header included.
#[test]
fn a_header_field_anchor_takes_nothing_but_a_single_value() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = setup(dir.path(), false);
    s.state
        .select_header_field(s.sheet, s.cols[0], HeaderField::LongName);
    s.transposed("1\t2\n3\t4");
    assert!(!s.state.can_undo(), "no history");
    assert!(s.state.column_meta(s.cols[0]).long_name.is_empty());
    s.transposed("Hello");
    assert_eq!(s.state.column_meta(s.cols[0]).long_name, "Hello");
    s.state.dispatch(Action::Undo);
    s.state.select_cell(s.sheet, s.cols[0], 1);
    s.transposed("42");
    assert_eq!(s.float(s.cols[0], 1), 42.0);
}

/// The x' cells of 3 columns, copied and pasted transposed on a cell, give a
/// column of 3 values. The other copied header rows are dropped and counted.
#[test]
fn an_x_prime_row_pastes_as_a_column_of_values() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = setup(dir.path(), false);
    for (column, x) in s.cols.clone().into_iter().zip(["1.5", "2.5", "3.5"]) {
        s.set_header(column, HeaderField::XPrime, x);
    }
    s.state
        .select_header_field(s.sheet, s.cols[0], HeaderField::XPrime);
    s.state
        .extend_header_field_selection(s.sheet, s.cols[2], HeaderField::XPrime);
    let text = s.state.copy_selection(s.sheet).unwrap();
    s.state.select_cell(s.sheet, s.cols[0], 0);
    let before = s.state.snapshot().history.undo_label.clone();
    s.transposed(&text);
    assert_eq!(
        [0, 1, 2].map(|row| s.float(s.cols[0], row)),
        [1.5, 2.5, 3.5]
    );
    assert_ne!(s.state.snapshot().history.undo_label, before);

    // x' and Long Name copied together: only the x' row goes.
    s.set_header(s.cols[0], HeaderField::LongName, "n");
    s.state
        .select_header_field(s.sheet, s.cols[0], HeaderField::XPrime);
    s.state
        .extend_header_field_selection(s.sheet, s.cols[2], HeaderField::LongName);
    let text = s.state.copy_selection(s.sheet).unwrap();
    s.state.select_cell(s.sheet, s.cols[1], 0);
    s.transposed(&text);
    assert_eq!(s.status().0, StatusLevel::Warning);
    assert!(
        s.status().1.contains("One header row other than x'"),
        "{:?}",
        s.status()
    );
}

/// Copied header fields with no x' row have nothing to paste transposed.
#[test]
fn header_fields_without_an_x_prime_row_say_so() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = setup(dir.path(), false);
    s.set_header(s.cols[0], HeaderField::LongName, "n");
    s.state
        .select_header_field(s.sheet, s.cols[0], HeaderField::LongName);
    s.state
        .extend_header_field_selection(s.sheet, s.cols[1], HeaderField::LongName);
    let text = s.state.copy_selection(s.sheet).unwrap();
    s.state.dispatch(Action::Undo);
    s.state.select_cell(s.sheet, s.cols[0], 0);
    s.transposed(&text);
    assert!(
        s.status().1.contains("Nothing to paste transposed"),
        "{:?}",
        s.status()
    );
    assert!(!s.state.can_undo());
}

/// A column pasted transposed onto an x' cell writes the x' of the successive
/// columns; a non-number is refused by itself and the others are written.
#[test]
fn a_column_pasted_on_an_x_prime_cell_fills_the_x_primes() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = setup(dir.path(), false);
    s.set_header(s.cols[1], HeaderField::XPrime, "9");
    let x = |s: &Setup, column| s.state.column_meta(column).x_prime;
    s.state
        .select_header_field(s.sheet, s.cols[0], HeaderField::XPrime);
    s.transposed("1\nx\n3");
    assert_eq!(x(&s, s.cols[0]), Some(1.0));
    assert_eq!(x(&s, s.cols[1]), Some(9.0), "the refused cell keeps its x'");
    assert_eq!(x(&s, s.cols[2]), Some(3.0));
    assert_eq!(s.status().0, StatusLevel::Error);
}

/// 4 values on the x' of the second-to-last column create two columns that
/// carry x'; the rows after the first are dropped and counted; one Undo.
#[test]
fn x_prime_values_beyond_the_edge_create_columns() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = setup(dir.path(), false);
    s.state
        .select_header_field(s.sheet, s.cols[1], HeaderField::XPrime);
    s.transposed("1\n2\n3\n4");
    let order = s.order();
    assert_eq!(order.len(), 5, "two columns created");
    let xs: Vec<Option<f64>> = order[1..]
        .iter()
        .map(|&column| s.state.column_meta(column).x_prime)
        .collect();
    assert_eq!(xs, [Some(1.0), Some(2.0), Some(3.0), Some(4.0)]);
    s.undo_back_to_nothing(3);

    // A second row has no x' to go to: dropped and counted.
    s.state
        .select_header_field(s.sheet, s.cols[0], HeaderField::XPrime);
    s.transposed("1\t2\n3\t4");
    assert!(
        s.status().1.contains("One header field did not fit"),
        "{:?}",
        s.status()
    );
    assert_eq!(s.state.column_meta(s.cols[0]).x_prime, Some(1.0));
}

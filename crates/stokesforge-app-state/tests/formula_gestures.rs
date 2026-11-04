//! Tests of the formula gestures of the application state (#412): full-column
//! copy / paste of formulas, the object clipboard, removing a formula, typing
//! in the header field, refreshing an imported column, a stale snapshot during
//! a long recalculation, and the column type of a formula column.
//!
//! Exploratory helpers print what they see with `eprintln!` (run with
//! `--nocapture`); the assertions pin what the documents promise.

use stokesforge_app_state::i18n::{self, Language};
use stokesforge_app_state::state::{Action, AppState, FullColumnsCopy, HeaderField};
use stokesforge_core::{Column, ColumnMeta, ColumnType, NodeId, Project};
use stokesforge_session::Session;

#[allow(dead_code)] // some handles are only used by some tests
struct World {
    _language: i18n::LanguageGuard,
    _session: Session,
    state: AppState,
    _dir: tempfile::TempDir,
    root: NodeId,
    book1: NodeId,
    book2: NodeId,
    sheet1: NodeId,
    sheet3: NodeId,
    blank: NodeId,
    sheet5: NodeId,
    /// `Folder/Book2/Sheet5`: a second workbook named `Book2`.
    twin_sheet5: NodeId,
    /// `Other/Book9/Sheet1`: in a folder that sees two `Book2`.
    far_sheet: NodeId,
    /// A second `Sheet3`, in `Book9`: the name exists twice in the project.
    far_sheet3: NodeId,
}

fn data(project: &mut Project, sheet: NodeId, name: &str, values: &[f64]) -> NodeId {
    project
        .add_column(sheet, None, Some(name), Column::from_f64(values))
        .unwrap()
}

fn world() -> World {
    let language = i18n::test_language(Language::English);
    let mut project = Project::new("P").unwrap();
    let root = project.root();
    let book1 = project.add_empty_workbook(root, Some("Book1")).unwrap();
    let sheet1 = project.add_worksheet(book1, Some("Sheet1")).unwrap();
    let sheet3 = project.add_worksheet(book1, Some("Sheet3")).unwrap();
    let blank = project.add_worksheet(book1, Some("Blanc 2")).unwrap();
    let book2 = project.add_empty_workbook(root, Some("Book2")).unwrap();
    let sheet5 = project.add_worksheet(book2, Some("Sheet5")).unwrap();
    let folder = project.add_folder(root, "Folder").unwrap();
    let twin = project.add_empty_workbook(folder, Some("Book2")).unwrap();
    let twin_sheet5 = project.add_worksheet(twin, Some("Sheet5")).unwrap();
    let other = project.add_folder(root, "Other").unwrap();
    let book9 = project.add_empty_workbook(other, Some("Book9")).unwrap();
    let far_sheet = project.add_worksheet(book9, Some("Sheet1")).unwrap();
    let far_sheet3 = project.add_worksheet(book9, Some("Sheet3")).unwrap();
    for sheet in [sheet1, far_sheet] {
        for (name, base) in [("A", 1.0), ("B", 10.0), ("C", 100.0), ("D", 1000.0)] {
            let values: Vec<f64> = (0..5).map(|i| base + f64::from(i)).collect();
            data(&mut project, sheet, name, &values);
        }
    }
    for (sheet, base) in [
        (sheet3, 3000.0),
        (blank, 4000.0),
        (sheet5, 5000.0),
        (twin_sheet5, 6000.0),
        (far_sheet3, 7000.0),
    ] {
        for (name, add) in [("A", 0.0), ("B", 100.0)] {
            let values: Vec<f64> = (0..5).map(|i| base + add + f64::from(i)).collect();
            data(&mut project, sheet, name, &values);
        }
    }
    // A is the X of B (the nearest X to its left) in both Sheet1 sheets.
    for sheet in [sheet1, far_sheet] {
        let a = project.tree().children(sheet).unwrap()[0];
        project
            .set_column_meta(
                a,
                ColumnMeta {
                    role: stokesforge_core::ColumnRole::X,
                    ..Default::default()
                },
            )
            .unwrap();
    }
    // `Sheet1!C` carries an x' that no user can type (infinite): a copy of it
    // pastes an x' that the destination refuses.
    let c = project.tree().children(sheet1).unwrap()[2];
    project
        .set_column_meta(
            c,
            ColumnMeta {
                x_prime: Some(f64::INFINITY),
                ..Default::default()
            },
        )
        .unwrap();
    let session = Session::spawn(project, None).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let state = AppState::new_with_preferences(
        session.handle(),
        dir.path().join("p.stf").display().to_string(),
        stokesforge_app_state::preferences::Preferences::load_from(None),
    )
    .unwrap();
    World {
        _language: language,
        _session: session,
        state,
        _dir: dir,
        root,
        book1,
        book2,
        sheet1,
        sheet3,
        blank,
        sheet5,
        twin_sheet5,
        far_sheet,
        far_sheet3,
    }
}

impl World {
    fn columns(&self, sheet: NodeId) -> Vec<NodeId> {
        self.state.tree().children(sheet).unwrap().to_vec()
    }

    /// Adds an empty column to `sheet` with the formula typed in the header.
    fn formula_column(&mut self, sheet: NodeId, text: &str) -> NodeId {
        self.state.dispatch(Action::AddColumn { sheet });
        let column = *self.columns(sheet).last().unwrap();
        self.type_formula(column, text);
        column
    }

    fn type_formula(&mut self, column: NodeId, text: &str) {
        self.state.begin_header_edit(column, HeaderField::Formula);
        self.state.header_editing_mut().unwrap().buffer = text.to_string();
        self.state.commit_header_edit();
    }

    fn letters(&self, column: NodeId) -> Option<String> {
        self.state.tree().formula_letters(column)
    }

    fn values(&self, column: NodeId) -> Vec<Option<f64>> {
        let node = self.state.tree().node(column).unwrap().as_column().unwrap();
        (0..node.len())
            .map(|row| Some(node.values().number_at(row)).filter(|v| v.is_finite()))
            .collect()
    }

    fn status(&self) -> String {
        self.state
            .status()
            .map(|s| format!("{}{}", if s.is_error() { "[error] " } else { "" }, s.text))
            .unwrap_or_default()
    }

    /// Ctrl+Shift+C on one column, "Formulas", returns the clipboard text.
    fn copy_formula_column(&mut self, sheet: NodeId, column: NodeId) -> String {
        self.state.select_column(sheet, column);
        let direct = self.state.copy_full_columns(sheet);
        assert!(direct.is_none(), "a formula column asks values or formulas");
        assert_eq!(self.state.pending_full_copy(), Some(sheet));
        self.state
            .dispatch(Action::ConfirmFullColumnsCopy(FullColumnsCopy::Formulas));
        self.state
            .take_object_clipboard_text()
            .expect("the copy is published")
    }

    /// Ctrl+V of `text` anchored on the X' field of `column`.
    fn paste_header_on(&mut self, sheet: NodeId, column: NodeId, text: &str) {
        self.state
            .select_header_field(sheet, column, HeaderField::XPrime);
        self.state.dispatch(Action::Paste {
            sheet,
            text: text.to_string(),
        });
    }

    /// Ctrl+Shift+C on several columns of `sheet`, then the confirmed `kind`;
    /// returns the clipboard text.
    fn copy_columns(&mut self, sheet: NodeId, columns: &[NodeId], kind: FullColumnsCopy) -> String {
        self.state.select_column(sheet, columns[0]);
        for &column in &columns[1..] {
            self.state.toggle_column_selection(sheet, column);
        }
        let _ = self.state.copy_full_columns(sheet);
        if self.state.pending_full_copy().is_some() {
            self.state.dispatch(Action::ConfirmFullColumnsCopy(kind));
        }
        self.state
            .take_object_clipboard_text()
            .expect("the copy is published")
    }

    /// `count` new empty columns at the end of `sheet`.
    fn new_columns(&mut self, sheet: NodeId, count: usize) -> Vec<NodeId> {
        for _ in 0..count {
            self.state.dispatch(Action::AddColumn { sheet });
        }
        let all = self.columns(sheet);
        all[all.len() - count..].to_vec()
    }

    fn undo_all_and_redo_all(&mut self) {
        let before = self.state.tree().clone();
        let mut steps = 0;
        while self.state.snapshot().history.can_undo {
            self.state.dispatch(Action::Undo);
            steps += 1;
            assert!(steps < 200);
        }
        for _ in 0..steps {
            self.state.dispatch(Action::Redo);
        }
        assert_eq!(
            *self.state.tree(),
            before,
            "redo walks back to the same tree"
        );
    }
}

const FORMULA_READ_ONLY: &str = "This column is computed by a formula";

/// The five formulas of these tests, typed in new columns of `Sheet1`.
fn sources(w: &mut World) -> Vec<(&'static str, NodeId)> {
    let sheet1 = w.sheet1;
    [
        "max(Sheet3!B)",
        "max([Book2]Sheet5!B)",
        "mean('Blanc 2'!B)",
        "mean(B, 1, 3)",
        "at(B, 1001, D)",
    ]
    .into_iter()
    .map(|text| (text, w.formula_column(sheet1, text)))
    .collect()
}

#[test]
fn sources_compute_what_the_tests_expect() {
    let mut w = world();
    let src = sources(&mut w);
    let expect: [f64; 5] = [3104.0, 5104.0, 4102.0, 11.0, 11.0];
    for ((text, column), want) in src.iter().zip(expect) {
        assert_eq!(w.letters(*column).as_deref(), Some(*text));
        let values = w.values(*column);
        assert_eq!(values[0], Some(want), "{text}: {values:?}");
    }
}

/// (letters, first value) the paste gives in `Sheet1`, `Sheet3`, and the
/// sheet `Other/Book9/Sheet1` (a project where `Book2` exists twice and
/// `Sheet3` exists in two workbooks); `None` letters = refused.
type Landing = (Option<&'static str>, Option<f64>);

#[test]
fn a_formulas_copy_reads_by_name_in_every_destination() {
    let mut w = world();
    let src = sources(&mut w);
    let (sheet1, sheet3, far) = (w.sheet1, w.sheet3, w.far_sheet);
    let table: [[Landing; 3]; 5] = [
        [
            (Some("max(Sheet3!B)"), Some(3104.0)),
            (Some("max(B)"), Some(3104.0)),
            // Book9 has its own Sheet3: the name resolves there.
            (Some("max(Sheet3!B)"), Some(7104.0)),
        ],
        [
            (Some("max([Book2]Sheet5!B)"), Some(5104.0)),
            (Some("max([Book2]Sheet5!B)"), Some(5104.0)),
            (None, None), // two workbooks named Book2: ambiguous
        ],
        [
            (Some("mean('Blanc 2'!B)"), Some(4102.0)),
            (Some("mean('Blanc 2'!B)"), Some(4102.0)),
            (None, None), // Book9 has no `Blanc 2`
        ],
        [
            (Some("mean(B, 1, 3)"), Some(11.0)),
            (Some("mean(B, 1, 3)"), None), // Sheet3 has no X column
            (Some("mean(B, 1, 3)"), Some(11.0)),
        ],
        [
            (Some("at(B, 1001, D)"), Some(11.0)),
            (Some("at(B, 1001, D)"), None),
            (Some("at(B, 1001, D)"), Some(11.0)),
        ],
    ];
    for ((text, column), row) in src.iter().zip(table) {
        let clip = w.copy_formula_column(sheet1, *column);
        for (sheet, (want_letters, want_value)) in [sheet1, sheet3, far].into_iter().zip(row) {
            w.state.dispatch(Action::AddColumn { sheet });
            let dst = *w.columns(sheet).last().unwrap();
            w.paste_header_on(sheet, dst, &clip);
            let place = w.state.tree().node(sheet).unwrap().name().to_string();
            assert_eq!(w.letters(dst).as_deref(), want_letters, "{text} -> {place}");
            if want_letters.is_some() {
                // The values are the formula's, never the pasted ones.
                assert_eq!(w.values(dst)[0], want_value, "{text} -> {place}");
            } else {
                let status = w.status();
                assert!(
                    status.starts_with("[error]")
                        && !status.contains(FORMULA_READ_ONLY)
                        && (status.contains("Several workbooks")
                            || status.contains("no sheet named")),
                    "{text} -> {place}: `{status}`"
                );
            }
        }
    }
}

#[test]
fn a_formulas_paste_is_one_history_entry() {
    let mut w = world();
    let src = sources(&mut w);
    let sheet3 = w.sheet3;
    let clip = w.copy_formula_column(w.sheet1, src[0].1);
    w.state.dispatch(Action::AddColumn { sheet: sheet3 });
    let dst = *w.columns(sheet3).last().unwrap();
    let before = w.state.tree().clone();
    w.paste_header_on(sheet3, dst, &clip);
    assert!(w.letters(dst).is_some());
    assert!(!w.status().starts_with("[error]"), "{}", w.status());
    let pasted = w.state.tree().clone();
    w.state.dispatch(Action::Undo);
    assert_eq!(*w.state.tree(), before, "one Undo restores the column");
    w.state.dispatch(Action::Redo);
    assert!(w.letters(dst).is_some());
    assert_eq!(
        *w.state.tree(),
        pasted,
        "Redo brings back formula and values"
    );
    w.undo_all_and_redo_all();
}

#[test]
fn a_successful_formulas_paste_does_not_end_on_an_error() {
    let mut w = world();
    let src = sources(&mut w);
    let sheet3 = w.sheet3;
    let clip = w.copy_formula_column(w.sheet1, src[0].1);
    w.state.dispatch(Action::AddColumn { sheet: sheet3 });
    let dst = *w.columns(sheet3).last().unwrap();
    w.paste_header_on(sheet3, dst, &clip);
    assert!(w.letters(dst).is_some());
    assert!(!w.status().starts_with("[error]"), "{}", w.status());
}

#[test]
fn a_formulas_paste_of_several_columns_keeps_the_values_of_the_ordinary_ones() {
    let mut w = world();
    let src = sources(&mut w);
    let (sheet1, sheet3) = (w.sheet1, w.sheet3);
    let ordinary = w.columns(sheet1)[1];
    let clip = w.copy_columns(sheet1, &[ordinary, src[0].1], FullColumnsCopy::Formulas);
    let dst = w.new_columns(sheet3, 2);
    let before = w.state.tree().clone();
    w.paste_header_on(sheet3, dst[0], &clip);
    assert_eq!(w.letters(dst[0]), None);
    assert_eq!(w.values(dst[0])[0], Some(10.0), "frozen values go in");
    assert_eq!(w.letters(dst[1]).as_deref(), Some("max(B)"));
    assert_eq!(w.values(dst[1])[0], Some(3104.0), "computed, not pasted");
    assert!(!w.status().starts_with("[error]"), "{}", w.status());
    let pasted = w.state.tree().clone();
    w.state.dispatch(Action::Undo);
    assert_eq!(*w.state.tree(), before, "one Undo restores both columns");
    w.state.dispatch(Action::Redo);
    assert_eq!(
        *w.state.tree(),
        pasted,
        "Redo brings back formula and values"
    );
}

#[test]
fn a_formulas_paste_anchored_on_the_formula_field_does_not_end_on_an_error() {
    let mut w = world();
    let src = sources(&mut w);
    let sheet3 = w.sheet3;
    let clip = w.copy_formula_column(w.sheet1, src[0].1);
    let dst = w.new_columns(sheet3, 1)[0];
    w.state
        .select_header_field(sheet3, dst, HeaderField::Formula);
    w.state.dispatch(Action::Paste {
        sheet: sheet3,
        text: clip,
    });
    assert_eq!(w.letters(dst).as_deref(), Some("max(B)"));
    assert_eq!(w.values(dst)[0], Some(3104.0));
    assert!(!w.status().starts_with("[error]"), "{}", w.status());
}

#[test]
fn a_formulas_paste_on_data_cells_replaces_the_formula_like_a_header_paste() {
    // Was `..._writes_values_and_keeps_the_read_only_rule`: since #432 S1 a
    // Formulas copy on a data cell acts as on the column's F(x)= field, so the
    // entry row of an ordinary column and a data cell of a formula column now
    // both end with the pasted formula and no error.
    let mut w = world();
    let src = sources(&mut w);
    let sheet3 = w.sheet3;
    let clip = w.copy_formula_column(w.sheet1, src[0].1);
    let b = w.new_columns(sheet3, 1)[0];
    let entry = w.state.entry_row(sheet3);
    w.state.select_cell(sheet3, b, entry);
    w.state.dispatch(Action::Paste {
        sheet: sheet3,
        text: clip.clone(),
    });
    assert_eq!(w.letters(b).as_deref(), Some("max(B)"));
    assert_eq!(w.values(b).len(), entry, "no row is created");
    assert!(!w.status().starts_with("[error]"), "{}", w.status());
    let f = w.formula_column(sheet3, "max(A)");
    w.state.select_cell(sheet3, f, 0);
    w.state.dispatch(Action::Paste {
        sheet: sheet3,
        text: clip,
    });
    assert_eq!(w.letters(f).as_deref(), Some("max(B)"));
    assert!(!w.status().starts_with("[error]"), "{}", w.status());
}

/// `Sheet3` gets a formula column C (`A*2`), copied as Formulas; returns it with
/// the clipboard text and one empty column D.
fn doubled_c(w: &mut World) -> (NodeId, String, NodeId) {
    let sheet3 = w.sheet3;
    let c = w.formula_column(sheet3, "A*2");
    let clip = w.copy_formula_column(sheet3, c);
    let d = w.new_columns(sheet3, 1)[0];
    (c, clip, d)
}

fn paste_on_cell(w: &mut World, column: NodeId, row: usize, clip: &str) {
    let sheet = w.sheet3;
    w.state.select_cell(sheet, column, row);
    w.state.dispatch(Action::Paste {
        sheet,
        text: clip.to_string(),
    });
}

#[test]
fn a_formulas_paste_on_a_data_cell_writes_the_formula_and_recomputes_from_row_one() {
    let mut w = world();
    let (c, clip, d) = doubled_c(&mut w);
    let before = w.state.tree().clone();
    paste_on_cell(&mut w, d, 4, &clip);
    assert_eq!(w.letters(d).as_deref(), Some("A*2"));
    assert_eq!(w.values(d), w.values(c));
    assert!(!w.status().starts_with("[error]"), "{}", w.status());
    w.state.dispatch(Action::Undo);
    assert_eq!(*w.state.tree(), before, "one Undo undoes the whole paste");
}

#[test]
fn a_formulas_paste_on_the_entry_row_writes_the_formula_without_extra_rows() {
    let mut w = world();
    let (c, clip, d) = doubled_c(&mut w);
    let entry = w.state.entry_row(w.sheet3);
    paste_on_cell(&mut w, d, entry, &clip);
    assert_eq!(w.letters(d).as_deref(), Some("A*2"));
    assert_eq!(w.values(d), w.values(c), "same length as C");
    assert!(!w.status().starts_with("[error]"), "{}", w.status());
}

#[test]
fn a_formulas_paste_of_two_columns_on_a_data_cell_keeps_the_plain_values_from_row_one() {
    let mut w = world();
    let sheet3 = w.sheet3;
    let b = w.columns(sheet3)[1];
    let c = w.formula_column(sheet3, "A*2");
    let clip = w.copy_columns(sheet3, &[b, c], FullColumnsCopy::Formulas);
    let dst = w.new_columns(sheet3, 2);
    paste_on_cell(&mut w, dst[0], 3, &clip);
    assert_eq!(w.letters(dst[0]), None);
    assert_eq!(w.values(dst[0]), w.values(b), "B's values from row 1");
    assert_eq!(w.letters(dst[1]).as_deref(), Some("A*2"));
    assert_eq!(w.values(dst[1]), w.values(c));
    assert!(!w.status().starts_with("[error]"), "{}", w.status());
}

#[test]
fn a_values_paste_on_a_data_cell_is_unchanged() {
    let mut w = world();
    let sheet3 = w.sheet3;
    let c = w.formula_column(sheet3, "A*2");
    let clip = w.copy_columns(sheet3, &[c], FullColumnsCopy::Values);
    let d = w.new_columns(sheet3, 1)[0];
    paste_on_cell(&mut w, d, 4, &clip);
    assert_eq!(w.letters(d), None, "no formula");
    let values = w.values(d);
    assert_eq!(values.len(), 9, "values start at row 4");
    assert_eq!(values[4], w.values(c)[0]);
    assert_eq!(values[0], None);
}

#[test]
fn a_values_paste_onto_a_formula_column_replaces_the_formula_by_the_values() {
    let mut w = world();
    let src = sources(&mut w);
    let (sheet1, sheet3) = (w.sheet1, w.sheet3);
    let clip = w.copy_columns(sheet1, &[src[0].1], FullColumnsCopy::Values);
    let dst = w.formula_column(sheet3, "max(A)");
    assert_eq!(w.values(dst)[0], Some(3004.0));
    w.paste_header_on(sheet3, dst, &clip);
    assert_eq!(w.letters(dst), None, "the formula is removed");
    assert_eq!(w.values(dst)[0], Some(3104.0), "the frozen values are in");
    assert!(!w.status().starts_with("[error]"), "{}", w.status());
}

#[test]
fn a_refused_formula_among_accepted_ones_ends_on_its_sentence_only() {
    let mut w = world();
    let src = sources(&mut w);
    let (sheet1, far) = (w.sheet1, w.far_sheet);
    let clip = w.copy_columns(sheet1, &[src[0].1, src[2].1], FullColumnsCopy::Formulas);
    let dst = w.new_columns(far, 2);
    w.paste_header_on(far, dst[0], &clip);
    assert_eq!(w.letters(dst[0]).as_deref(), Some("max(Sheet3!B)"));
    assert_eq!(w.values(dst[0])[0], Some(7104.0));
    assert_eq!(w.letters(dst[1]), None, "Book9 has no `Blanc 2`");
    let status = w.status();
    assert!(
        status.starts_with("[error]") && status.contains("no sheet named"),
        "{status}"
    );
    // The status names the column whose formula was not pasted (#433).
    let letters = w.state.tree().column_letters(dst[1]).unwrap();
    assert!(
        status.contains(&format!("column {letters} ")),
        "no column name in `{status}`"
    );
    assert!(!status.contains(FORMULA_READ_ONLY), "{status}");
}

#[test]
fn an_accepted_formula_after_a_refused_x_prime_keeps_its_computed_values() {
    let mut w = world();
    let src = sources(&mut w);
    let (sheet1, sheet3) = (w.sheet1, w.sheet3);
    // `Sheet1!C` holds an x' that is not a finite number (see `world`): it is
    // refused at the destination while the next column's formula is accepted.
    let c = w.columns(sheet1)[2];
    let clip = w.copy_columns(sheet1, &[c, src[0].1], FullColumnsCopy::Formulas);
    let dst = w.new_columns(sheet3, 2);
    // An error already on the status bar must not count as a failure of the
    // paste.
    w.state.dispatch(Action::SetColumnXPrime {
        column: dst[1],
        text: "abc".to_string(),
    });
    assert!(w.status().starts_with("[error]"), "{}", w.status());
    w.paste_header_on(sheet3, dst[0], &clip);
    assert_eq!(w.letters(dst[1]).as_deref(), Some("max(B)"));
    assert_eq!(w.values(dst[1])[0], Some(3104.0), "computed, not pasted");
    // The refused x' is reported, then overwritten by the notice of the
    // other column's pasted values; the read-only error must never appear.
    let status = w.status();
    assert!(!status.contains(FORMULA_READ_ONLY), "{status}");
}

#[test]
fn pasting_a_formula_that_closes_a_loop_is_refused_with_the_loop_sentence() {
    let mut w = world();
    let src = sources(&mut w); // Sheet1!E = max(Sheet3!B)
    let (sheet1, sheet3) = (w.sheet1, w.sheet3);
    let e = src[0].1;
    let z = w.formula_column(sheet3, "max(Sheet1!E)");
    assert!(w.letters(z).is_some(), "{}", w.status());
    let b3 = w.columns(sheet3)[1];
    let clip = w.copy_formula_column(sheet3, z);
    w.paste_header_on(sheet3, b3, &clip);
    assert_eq!(w.letters(b3), None, "a loop is never stored");
    assert!(
        w.status()
            .contains("Formulas refer to each other in a loop"),
        "{}",
        w.status()
    );
    assert_eq!(w.letters(e).as_deref(), Some("max(Sheet3!B)"));
    let _ = sheet1;
}

#[test]
fn a_refused_formulas_paste_leaves_the_destination_data_alone() {
    let mut w = world();
    let _ = sources(&mut w);
    let sheet3 = w.sheet3;
    let z = w.formula_column(sheet3, "max(Sheet1!E)");
    let b3 = w.columns(sheet3)[1];
    let before = w.values(b3);
    let clip = w.copy_formula_column(sheet3, z);
    w.paste_header_on(sheet3, b3, &clip);
    assert_eq!(w.letters(b3), None);
    assert_eq!(w.values(b3), before);
}

// -- object clipboard --

struct Objects {
    _language: Option<i18n::LanguageGuard>,
    _session: Session,
    _dir: tempfile::TempDir,
    state: AppState,
    root: NodeId,
    folder: NodeId,
    book_a: NodeId,
    book_b: NodeId,
    sheet_a1: NodeId,
}

fn objects() -> Objects {
    objects_with(Some(i18n::test_language(Language::English)))
}

/// The language guard is a process-wide lock: a second project in the same
/// test must not take it again.
fn objects_with(language: Option<i18n::LanguageGuard>) -> Objects {
    let mut project = Project::new("O").unwrap();
    let root = project.root();
    let folder = project.add_folder(root, "Src").unwrap();
    let book_a = project.add_empty_workbook(folder, Some("BookA")).unwrap();
    let s1 = project.add_worksheet(book_a, Some("S1")).unwrap();
    let s2 = project.add_worksheet(book_a, Some("S2")).unwrap();
    let book_b = project.add_empty_workbook(root, Some("BookB")).unwrap();
    let t1 = project.add_worksheet(book_b, Some("T1")).unwrap();
    data(&mut project, s1, "A", &[1.0, 2.0]);
    data(&mut project, s2, "A", &[5.0, 7.0]);
    data(&mut project, t1, "A", &[100.0, 200.0]);
    for (name, text) in [
        ("L", "A*2"),
        ("R", "max(S2!A)"),
        ("O", "max([BookB]T1!A)"),
        ("M", "B + C"),
    ] {
        let id = data(&mut project, s1, name, &[0.0, 0.0]);
        let formula = project.tree().formula_for_entry(id, text).unwrap();
        project
            .set_column_meta(
                id,
                ColumnMeta {
                    formula: Some(formula),
                    ..Default::default()
                },
            )
            .unwrap();
    }
    let session = Session::spawn(project, None).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let state = AppState::new_with_preferences(
        session.handle(),
        dir.path().join("o.stf").display().to_string(),
        stokesforge_app_state::preferences::Preferences::load_from(None),
    )
    .unwrap();
    Objects {
        _language: language,
        _session: session,
        _dir: dir,
        state,
        root,
        folder,
        book_a,
        book_b,
        sheet_a1: s1,
    }
}

/// For the formula columns L, R, O, M of the first sheet of `book`: what each
/// reads, as (letters, where). Where: `copy` = inside `scope_copy`,
/// `original` = inside `scope_original`, `other` = elsewhere, `none` = reads
/// no column (`#REF`).
fn reads(
    state: &AppState,
    book: NodeId,
    copy_scope: NodeId,
    original_scope: NodeId,
) -> Vec<(String, &'static str, Vec<Option<f64>>)> {
    let tree = state.tree();
    let in_copy: Vec<NodeId> = tree.walk(copy_scope).unwrap();
    let in_original: Vec<NodeId> = tree.walk(original_scope).unwrap();
    let sheet = tree.children(book).unwrap()[0];
    let columns = tree.children(sheet).unwrap().to_vec();
    columns[1..]
        .iter()
        .map(|&column| {
            let node = tree.node(column).unwrap().as_column().unwrap();
            let formula = node.meta().formula.clone().expect("formula kept");
            let read: Vec<NodeId> = formula.columns().into_iter().collect();
            let mut places: Vec<&'static str> = read
                .iter()
                .map(|id| {
                    if in_copy.contains(id) {
                        "copy"
                    } else if in_original.contains(id) {
                        "original"
                    } else if tree.node(*id).is_some() {
                        "other"
                    } else {
                        "gone"
                    }
                })
                .collect();
            places.sort_unstable();
            places.dedup();
            let place = match places.as_slice() {
                [] => "none",
                [one] => one,
                _ => "mixed",
            };
            let values = (0..node.len())
                .map(|row| Some(node.values().number_at(row)).filter(|v| v.is_finite()))
                .collect();
            (tree.formula_letters(column).unwrap(), place, values)
        })
        .collect()
}

impl Objects {
    fn copy_and_paste(&mut self, ids: Vec<NodeId>, parent: NodeId) -> NodeId {
        self.state.dispatch(Action::CopyObjects(ids));
        let text = self.state.take_object_clipboard_text().expect("copied");
        self.state.dispatch(Action::PasteObjects { parent, text });
        self.state.take_copied_objects()[0].1
    }
}

#[test]
fn a_pasted_folder_reads_its_own_copy_and_the_original_outside() {
    let mut o = objects();
    // The fixture itself.
    let sheet_a1 = o.sheet_a1;
    let _ = sheet_a1;
    let (folder, root) = (o.folder, o.root);
    let original = reads(&o.state, o.book_a, folder, folder);
    assert_eq!(original[0].0, "A*2");

    // Into a new folder.
    o.state.dispatch(Action::AddFolder { parent: root });
    let dest = *o.state.tree().children(root).unwrap().last().unwrap();
    let pasted = o.copy_and_paste(vec![folder], dest);
    let copied_book = o.state.tree().children(pasted).unwrap()[0];
    let got = reads(&o.state, copied_book, pasted, folder);
    assert_eq!(
        got.iter()
            .map(|(l, p, _)| (l.as_str(), *p))
            .collect::<Vec<_>>(),
        [
            ("A*2", "copy"),
            ("max(S2!A)", "copy"),
            // BookB is outside the copy: the copy keeps reading the original.
            ("max([BookB]T1!A)", "other"),
            ("B + C", "copy"),
        ]
    );
    assert_eq!(got[0].2, [Some(2.0), Some(4.0)]);
    assert_eq!(got[1].2[0], Some(7.0));
    assert_eq!(got[2].2[0], Some(200.0));
    assert_eq!(got[3].2[0], Some(9.0));
}

#[test]
fn pasting_a_folder_twice_in_its_own_parent_gives_two_independent_copies() {
    let mut o = objects();
    let (folder, root) = (o.folder, o.root);
    o.state.dispatch(Action::CopyObjects(vec![folder]));
    let text = o.state.take_object_clipboard_text().unwrap();
    let mut pasted = Vec::new();
    for _ in 0..2 {
        o.state.dispatch(Action::PasteObjects {
            parent: root,
            text: text.clone(),
        });
        pasted.push(o.state.take_copied_objects()[0].1);
    }
    let names: Vec<String> = pasted
        .iter()
        .map(|id| o.state.tree().node(*id).unwrap().name().to_string())
        .collect();
    assert_ne!(names[0], names[1]);
    assert!(!names.contains(&"Src".to_string()), "{names:?}");
    for id in &pasted {
        let book = o.state.tree().children(*id).unwrap()[0];
        let got = reads(&o.state, book, *id, folder);
        assert!(
            got.iter()
                .all(|(_, place, _)| matches!(*place, "copy" | "other")),
            "{got:?}"
        );
    }
    // Each pasted folder reads **its own** sheets.
    let a = o.state.tree().children(pasted[0]).unwrap()[0];
    let b = o.state.tree().children(pasted[1]).unwrap()[0];
    let ra = reads(&o.state, a, pasted[0], pasted[1]);
    let rb = reads(&o.state, b, pasted[1], pasted[0]);
    assert!(
        ra.iter().chain(&rb).all(|(_, p, _)| *p != "original"),
        "{ra:?} {rb:?}"
    );
    // One undo removes one paste.
    o.state.dispatch(Action::Undo);
    assert_eq!(o.state.tree().children(root).unwrap().len(), 3);
}

#[test]
fn duplicating_a_workbook_and_a_sheet_keeps_local_reads_in_the_copy() {
    let mut o = objects();
    let (book_a, root) = (o.book_a, o.root);
    // A lone workbook copied into the root: its sheet S2 comes along, BookB
    // does not.
    let pasted = o.copy_and_paste(vec![book_a], root);
    let got = reads(&o.state, pasted, pasted, book_a);
    assert_eq!(
        got.iter()
            .map(|(l, p, _)| (l.as_str(), *p))
            .collect::<Vec<_>>(),
        [
            ("A*2", "copy"),
            ("max(S2!A)", "copy"),
            ("max([BookB]T1!A)", "other"),
            ("B + C", "copy"),
        ]
    );
    // A sheet duplicated in place (copying a sheet alone goes through the
    // sheet duplication): L reads the copy, R reads the copy of S2? S2 is a
    // sibling that is NOT copied, so it keeps reading the original.
    let sheet = o.sheet_a1;
    let s2 = o.state.tree().children(o.book_a).unwrap()[1];
    let before = o.state.tree().children(o.book_a).unwrap().to_vec();
    o.state.dispatch(Action::DuplicateSheet(sheet));
    let after = o.state.tree().children(o.book_a).unwrap().to_vec();
    let copied_sheet = *after
        .iter()
        .find(|id| !before.contains(id))
        .expect("a copy");
    let columns = o.state.tree().children(copied_sheet).unwrap().to_vec();
    let letters: Vec<String> = columns[1..]
        .iter()
        .map(|c| {
            o.state
                .tree()
                .formula_letters(*c)
                .unwrap_or_else(|| "<none>".into())
        })
        .collect();
    assert_eq!(letters, ["A*2", "max(S2!A)", "max([BookB]T1!A)", "B + C"]);
    let tree = o.state.tree();
    let read_by = |index: usize| -> Vec<NodeId> {
        let node = tree.node(columns[index]).unwrap().as_column().unwrap();
        node.meta()
            .formula
            .as_ref()
            .unwrap()
            .columns()
            .into_iter()
            .collect()
    };
    let in_copy = |id: &NodeId| tree.parent(*id).unwrap() == Some(copied_sheet);
    assert!(read_by(1).iter().all(in_copy), "L reads the copy");
    assert!(read_by(4).iter().all(in_copy), "M reads the copy");
    assert!(
        read_by(2)
            .iter()
            .all(|id| tree.parent(*id).unwrap() == Some(s2)),
        "R keeps reading the original S2 (not copied)"
    );
}

/// Pasted in **another project**, a formula keeps what comes along with the
/// copy and gets `#REF` for what stays behind, even if that project has the
/// same names (its identifiers mean nothing there).
#[test]
fn a_folder_pasted_in_another_project_voids_only_what_stayed_behind() {
    let mut o = objects();
    o.state.dispatch(Action::CopyObjects(vec![o.folder]));
    let text = o.state.take_object_clipboard_text().unwrap();
    let mut other = objects_with(None);
    let root = other.root;
    other
        .state
        .dispatch(Action::PasteObjects { parent: root, text });
    let pasted = other.state.take_copied_objects()[0].1;
    let book = other.state.tree().children(pasted).unwrap()[0];
    let got = reads(&other.state, book, pasted, other.folder);
    assert_eq!(
        got.iter()
            .map(|(l, p, _)| (l.as_str(), *p))
            .collect::<Vec<_>>(),
        [
            ("A*2", "copy"),
            ("max(S2!A)", "copy"),
            ("max(#REF)", "gone"),
            ("B + C", "copy"),
        ]
    );
    assert_eq!(got[2].2[0], None, "the voided read computes nothing");
    assert_eq!(got[1].2[0], Some(7.0));
    // The target project's own BookB is untouched.
    let _ = other.book_b;
}

// -- removing a formula --

/// `E = A*2` and its reader `F = E + 1` in `Sheet1` (A = 1..5).
fn chain(w: &mut World) -> (NodeId, NodeId) {
    let sheet1 = w.sheet1;
    let e = w.formula_column(sheet1, "A*2");
    let f = w.formula_column(sheet1, "E + 1");
    assert_eq!(w.values(e), [2.0, 4.0, 6.0, 8.0, 10.0].map(Some));
    assert_eq!(w.values(f), [3.0, 5.0, 7.0, 9.0, 11.0].map(Some));
    (e, f)
}

fn walk_undo_redo(w: &mut World, before: &std::sync::Arc<stokesforge_core::Tree>) {
    let after = w.state.tree().clone();
    w.state.dispatch(Action::Undo);
    assert_eq!(
        **w.state.tree(),
        **before,
        "one Undo gives back formula and data"
    );
    w.state.dispatch(Action::Redo);
    assert_eq!(**w.state.tree(), *after, "Redo replays it");
}

#[test]
fn emptying_the_formula_field_drops_the_values_and_the_reader_follows() {
    let mut w = world();
    let (e, f) = chain(&mut w);
    let before = w.state.tree().clone();
    w.type_formula(e, "");
    assert_eq!(w.letters(e), None);
    assert!(w.values(e).iter().all(Option::is_none), "{:?}", w.values(e));
    assert!(
        w.values(f).iter().all(Option::is_none),
        "reader recalculated on the dropped data: {:?}",
        w.values(f)
    );
    walk_undo_redo(&mut w, &before);
    w.state.dispatch(Action::Undo);
    assert_eq!(w.values(f), [3.0, 5.0, 7.0, 9.0, 11.0].map(Some));
}

#[test]
fn removing_a_formula_keeping_the_data_leaves_the_reader_on_the_kept_values() {
    let mut w = world();
    let (e, f) = chain(&mut w);
    let sheet1 = w.sheet1;
    let before = w.state.tree().clone();
    w.state.select_header_field(sheet1, e, HeaderField::Formula);
    w.state
        .dispatch(Action::RemoveFormulaKeepData { sheet: sheet1 });
    assert_eq!(w.letters(e), None);
    assert_eq!(w.values(e), [2.0, 4.0, 6.0, 8.0, 10.0].map(Some));
    assert_eq!(w.values(f), [3.0, 5.0, 7.0, 9.0, 11.0].map(Some));
    // The kept column is plain data now: editing it moves the reader.
    w.state.select_cell(sheet1, e, 0);
    w.state.dispatch(Action::Paste {
        sheet: sheet1,
        text: "100".into(),
    });
    assert_eq!(w.values(e)[0], Some(100.0), "{}", w.status());
    assert_eq!(w.values(f)[0], Some(101.0), "reader follows the edit");
    w.state.dispatch(Action::Undo);
    walk_undo_redo(&mut w, &before);
}

#[test]
fn backspace_in_the_formula_field_drops_the_values() {
    let mut w = world();
    let (e, f) = chain(&mut w);
    let sheet1 = w.sheet1;
    let before = w.state.tree().clone();
    w.state.select_header_field(sheet1, e, HeaderField::Formula);
    w.state
        .dispatch(Action::ClearSelectedCells { sheet: sheet1 });
    assert_eq!(w.letters(e), None);
    assert!(w.values(e).iter().all(Option::is_none), "{:?}", w.values(e));
    assert!(w.values(f).iter().all(Option::is_none), "{:?}", w.values(f));
    walk_undo_redo(&mut w, &before);
}

#[test]
fn removing_the_formulas_of_two_columns_at_once_is_one_undo() {
    let mut w = world();
    let (e, f) = chain(&mut w);
    let sheet1 = w.sheet1;
    let before = w.state.tree().clone();
    w.state.select_header_field(sheet1, e, HeaderField::Formula);
    w.state
        .extend_header_field_selection(sheet1, f, HeaderField::Formula);
    w.state
        .dispatch(Action::RemoveFormulaKeepData { sheet: sheet1 });
    assert_eq!((w.letters(e), w.letters(f)), (None, None));
    assert_eq!(w.values(f), [3.0, 5.0, 7.0, 9.0, 11.0].map(Some));
    walk_undo_redo(&mut w, &before);
}

// -- typing --

fn new_target(w: &mut World) -> NodeId {
    let sheet1 = w.sheet1;
    w.state.dispatch(Action::AddColumn { sheet: sheet1 });
    *w.columns(sheet1).last().unwrap() // column E
}

#[test]
fn badly_written_formulas_are_kept_framed_and_say_why_in_a_sentence() {
    let mut w = world();
    let target = new_target(&mut w);
    let cases: [(&str, &str); 11] = [
        ("frob(A)", "frob"),
        ("sin(A, B)", "argument"),
        ("sin()", "argument"),
        ("E + 1", "own column"),
        ("Sheet3!B", "can only be read by"),
        ("max(Nope!A)", "Nope"),
        ("max([Nobook]Sheet1!A)", "Nobook"),
        ("max(Sheet3!B, Sheet3!A)", "can only be read by"),
        ("at(Sheet3!B, 1, Sheet3!A)", "without a prefix"),
        ("1e400*A", "too large"),
        ("A*1e-400", "too small"),
    ];
    for (text, hint) in cases {
        w.type_formula(target, text);
        // Kept (#433): the text stays, the column is empty, the cell is framed.
        assert!(w.letters(target).is_some(), "`{text}` was dropped");
        assert!(w.values(target).iter().all(Option::is_none), "`{text}`");
        let status = w.status();
        assert!(status.starts_with("[error]"), "`{text}`: `{status}`");
        for raw in ["Parse(", "UnknownFunction", "TooDeep", "{", "\\u{"] {
            assert!(!status.contains(raw), "`{text}`: raw text in `{status}`");
        }
        assert!(status.contains(hint), "`{text}`: `{status}` lacks `{hint}`");
        let issue = w.state.formula_issue(target).expect("framed");
        assert!(issue.contains(hint), "`{text}`: frame says `{issue}`");
    }
}

/// A number outside the f64 range is flagged as soon as it is typed
/// (`header_edit_issue`), and a stored text keeps its number (#429).
#[test]
fn a_number_out_of_range_is_refused_while_typing() {
    let mut w = world();
    let target = new_target(&mut w);
    w.state.begin_header_edit(target, HeaderField::Formula);
    w.state.header_editing_mut().unwrap().buffer = "1e400*A".to_string();
    let refusal = w.state.header_edit_issue().expect("refused while typing");
    assert!(
        refusal.contains("1e400") && refusal.contains("too large"),
        "{refusal}"
    );
    assert!(!refusal.contains("NumberOutOfRange"), "{refusal}");
    w.state.header_editing_mut().unwrap().buffer = "1e308*A".to_string();
    assert_eq!(w.state.header_edit_issue(), None);
}

/// A formula with a syntax error is kept, the column has no values until it is
/// fixed, the status says so and the cell is framed (#433).
#[test]
fn a_formula_with_a_syntax_error_is_kept_and_the_column_is_empty() {
    let mut w = world();
    let target = new_target(&mut w);
    let deep = format!("{}A{}", "(".repeat(1001), ")".repeat(1001));
    let very_deep = format!("{}A", "-".repeat(100_000));
    for text in ["A +", "(A", "A ## B", &deep, &very_deep] {
        w.type_formula(target, text);
        let kept = w.letters(target);
        assert_eq!(
            kept.as_deref(),
            Some(text),
            "`{}`",
            text.chars().take(10).collect::<String>()
        );
        assert!(w.values(target).iter().all(Option::is_none));
        assert!(w.status().starts_with("[error]"), "{}", w.status());
    }
}

#[test]
fn a_kept_formula_that_does_not_parse_tells_the_user_why() {
    let mut w = world();
    let target = new_target(&mut w);
    w.type_formula(target, "A +");
    assert!(w.letters(target).is_some());
    let issue = w.state.formula_issue(target).expect("framed");
    assert!(issue.contains("incomplete or not valid"), "{issue}");
    assert!(w.status().starts_with("[error]"), "{}", w.status());
    // The frame holds no parser detail.
    assert!(
        !issue.contains("token") && !issue.contains("Parse"),
        "{issue}"
    );
}

#[test]
fn a_mutual_loop_and_a_self_loop_are_kept_framed_across_sheets() {
    let mut w = world();
    let sheet1 = w.sheet1;
    let sheet3 = w.sheet3;
    let a = w.formula_column(sheet1, "max(Sheet3!A)"); // E reads Sheet3!A
    assert!(w.letters(a).is_some());
    let a3 = w.columns(sheet3)[0];
    w.type_formula(a3, "max(Sheet1!E)");
    assert_eq!(w.letters(a3).as_deref(), Some("max(Sheet1!E)"), "kept");
    assert!(w.status().contains("loop"), "{}", w.status());
    assert!(w.state.formula_issue(a3).unwrap().contains("loop"));
    w.type_formula(a, "max(Sheet1!E)");
    assert!(
        w.status().contains("own column"),
        "a self reference says so: {}",
        w.status()
    );
    // Shown without the prefix of its own sheet.
    assert_eq!(w.letters(a).as_deref(), Some("max(E)"), "kept");
    assert!(w.values(a).iter().all(Option::is_none), "uncomputed");
}

/// Typing a text the field cannot read keeps it, and one Undo gives back the
/// old formula **and** its values (#433).
#[test]
fn a_kept_formula_is_undone_with_the_old_formula_and_values() {
    let mut w = world();
    let (e, _) = chain(&mut w);
    let before = w.state.tree().clone();
    assert_eq!(w.values(e), [2.0, 4.0, 6.0, 8.0, 10.0].map(Some));
    w.type_formula(e, "A +");
    assert_eq!(w.letters(e).as_deref(), Some("A +"));
    assert!(w.values(e).iter().all(Option::is_none));
    w.state.dispatch(Action::Undo);
    assert_eq!(w.letters(e).as_deref(), Some("A*2"));
    assert_eq!(w.values(e), [2.0, 4.0, 6.0, 8.0, 10.0].map(Some));
    assert_eq!(**w.state.tree(), *before, "the whole tree is back");
}

/// The frame depends on the stored text, not on the rows: a sheet with no row
/// shows it too (#433).
#[test]
fn a_kept_formula_in_a_sheet_of_no_row_is_framed() {
    let mut w = world();
    let book1 = w.book1;
    w.state.dispatch(Action::AddWorksheet { book: book1 });
    let sheet = *w.state.tree().children(book1).unwrap().last().unwrap();
    let target = w.formula_column(sheet, "1 +");
    assert_eq!(w.letters(target).as_deref(), Some("1 +"));
    let issue = w.state.formula_issue(target).expect("framed with no row");
    assert!(issue.contains("incomplete or not valid"), "{issue}");
}

/// A bound formula whose workbook name became ambiguous (a second `Book2` in
/// another folder, as an old file can hold) computes by identifier and is
/// **not** framed: the frame re-reads the text only when it does not compile
/// (#433). Witness: re-reading every formula frames it as ambiguous.
#[test]
fn a_formula_that_compiles_is_not_framed_when_its_names_became_ambiguous() {
    let _language = i18n::test_language(Language::English);
    let mut project = Project::new("Twins").unwrap();
    let root = project.root();
    let book = project.add_empty_workbook(root, Some("Book1")).unwrap();
    let sheet1 = project.add_worksheet(book, Some("Sheet1")).unwrap();
    let folder_a = project.add_folder(root, "FolderA").unwrap();
    let book2 = project.add_empty_workbook(folder_a, Some("Book2")).unwrap();
    let sheet5 = project.add_worksheet(book2, Some("Sheet5")).unwrap();
    data(&mut project, sheet1, "A", &[1.0, 2.0]);
    data(&mut project, sheet5, "A", &[5.0, 7.0]);
    let target = data(&mut project, sheet1, "F", &[0.0]);
    let formula = project
        .tree()
        .formula_for_entry(target, "max([Book2]Sheet5!A)")
        .unwrap();
    project
        .set_column_meta(
            target,
            ColumnMeta {
                formula: Some(formula),
                ..Default::default()
            },
        )
        .unwrap();
    let folder_b = project.add_folder(root, "FolderB").unwrap();
    let twin = project.add_empty_workbook(folder_b, Some("Book2")).unwrap();
    let twin_sheet = project.add_worksheet(twin, Some("Sheet5")).unwrap();
    data(&mut project, twin_sheet, "A", &[9.0]);
    assert!(
        project
            .tree()
            .formula_for_entry(target, "max([Book2]Sheet5!A)")
            .is_err(),
        "the text read again is ambiguous now"
    );
    let session = Session::spawn(project, None).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let state = AppState::new_with_preferences(
        session.handle(),
        dir.path().join("p.stf").display().to_string(),
        stokesforge_app_state::preferences::Preferences::load_from(None),
    )
    .unwrap();
    assert_eq!(state.formula_issue(target), None);
}

/// A range written with a decimal comma keeps its range, is rewritten, and the
/// field says so in orange: a warning on the status bar, never red (#433).
#[test]
fn a_comma_range_is_rewritten_and_warned_not_framed() {
    let mut w = world();
    let target = new_target(&mut w);
    w.type_formula(target, "max(B,1,5)");
    assert_eq!(w.letters(target).as_deref(), Some("max(B, 1, 5)"));
    let status = w.state.status().unwrap().clone();
    assert!(!status.is_error(), "{}", status.text);
    assert!(
        status.text.contains("from 1 to 5") && status.text.contains("1.5"),
        "{}",
        status.text
    );
    assert_eq!(w.state.formula_issue(target), None, "not red");
    assert_eq!(
        w.state.take_formula_warning(target).as_deref(),
        Some(status.text.as_str()),
        "handed to the grid once"
    );
    assert_eq!(w.state.take_formula_warning(target), None);

    // Already spaced, or already a range with spaces: nothing to say.
    w.type_formula(target, "max(B, 1, 5)");
    assert_eq!(w.state.take_formula_warning(target), None);

    w.type_formula(target, "max(B, 2, 4)");
    assert_eq!(w.state.take_formula_warning(target), None);
    w.type_formula(target, "max(B,2,4)");
    assert_eq!(w.letters(target).as_deref(), Some("max(B, 2, 4)"));
    assert!(w.state.take_formula_warning(target).is_some());
}

#[test]
fn the_comma_rewrite_leaves_other_calls_as_typed_and_red() {
    let mut w = world();
    let target = new_target(&mut w);
    for (text, hint) in [("at(B,1,5)", "whole column"), ("round(B,1,5)", "takes")] {
        w.type_formula(target, text);
        assert_eq!(w.letters(target).as_deref(), Some(text), "as typed");
        assert_eq!(w.state.take_formula_warning(target), None, "`{text}`");
        let issue = w.state.formula_issue(target).expect("red");
        assert!(issue.contains(hint), "`{text}`: {issue}");
        assert!(w.status().starts_with("[error]"), "`{text}`");
    }
    for text in ["atan2(1,5)", "if(A>0,1,5)"] {
        w.type_formula(target, text);
        assert_eq!(w.letters(target).as_deref(), Some(text), "as typed");
        assert_eq!(w.state.take_formula_warning(target), None, "`{text}`");
    }
}

// -- refresh from the source file --

struct Imported {
    _language: Option<i18n::LanguageGuard>,
    _session: Session,
    state: AppState,
    dir: tempfile::TempDir,
    sheet: NodeId,
    x: NodeId,
    y: NodeId,
}

fn imported(with_formulas: bool, first: bool) -> Imported {
    let language = first.then(|| i18n::test_language(Language::English));
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("m.csv");
    std::fs::write(&file, "1;10\n2;20\n3;30\n").unwrap();
    let mut recipe = stokesforge_import::ImportRecipe::new("semicolon");
    recipe.separator = stokesforge_import::Separator::Semicolon;
    let mut project = Project::new("I").unwrap();
    let root = project.root();
    let book = project.add_empty_workbook(root, Some("Book")).unwrap();
    let sheet = project.add_worksheet(book, Some("m")).unwrap();
    let add = |project: &mut Project, name: &str, values: &[f64], column: usize| {
        let id = data(project, sheet, name, values);
        project
            .set_column_meta(
                id,
                ColumnMeta {
                    import_source: Some(stokesforge_core::ImportSource {
                        path: file.display().to_string(),
                        recipe_json: recipe.to_json(),
                        column,
                        block: 0,
                    }),
                    ..Default::default()
                },
            )
            .unwrap();
        id
    };
    let x = add(&mut project, "X", &[1.0, 2.0, 3.0], 0);
    let y = add(&mut project, "Y", &[10.0, 20.0, 30.0], 1);
    if with_formulas {
        for (name, text) in [("Z", "B * 2"), ("W", "C + A")] {
            let id = data(&mut project, sheet, name, &[0.0]);
            let formula = project.tree().formula_for_entry(id, text).unwrap();
            project
                .set_column_meta(
                    id,
                    ColumnMeta {
                        formula: Some(formula),
                        ..Default::default()
                    },
                )
                .unwrap();
        }
    }
    let session = Session::spawn(project, None).unwrap();
    let mut state = AppState::new_with_preferences(
        session.handle(),
        dir.path().join("i.stf").display().to_string(),
        stokesforge_app_state::preferences::Preferences::load_from(None),
    )
    .unwrap();
    // A spawned project holds the values it was built with; any command makes
    // the actor recalculate the formula columns (a loaded file does at load).
    state.dispatch(Action::AddColumn { sheet });
    state.dispatch(Action::Undo);
    Imported {
        _language: language,
        _session: session,
        state,
        dir,
        sheet,
        x,
        y,
    }
}

impl Imported {
    fn rewrite(&self, text: &str) {
        std::fs::write(self.dir.path().join("m.csv"), text).unwrap();
    }

    fn column(&self, index: usize) -> (NodeId, Vec<Option<f64>>) {
        let id = self.state.tree().children(self.sheet).unwrap()[index];
        let node = self.state.tree().node(id).unwrap().as_column().unwrap();
        let values = (0..node.len())
            .map(|row| Some(node.values().number_at(row)).filter(|v| v.is_finite()))
            .collect();
        (id, values)
    }
}

#[test]
fn refreshing_an_imported_sheet_keeps_its_formula_columns_following() {
    let mut i = imported(true, true);
    let sheet = i.sheet;
    assert_eq!(i.column(2).1, [20.0, 40.0, 60.0].map(Some).to_vec());
    let before = i.state.tree().clone();
    // Longer file.
    i.rewrite("5;50\n6;60\n7;70\n8;80\n9;90\n");
    i.state.dispatch(Action::RefreshFromSource { sheet });
    assert!(
        !i.state.status().is_some_and(|s| s.is_error()),
        "{:?}",
        i.state.status()
    );
    assert_eq!(i.column(0).1, [5.0, 6.0, 7.0, 8.0, 9.0].map(Some).to_vec());
    assert_eq!(
        i.column(2).1,
        [100.0, 120.0, 140.0, 160.0, 180.0].map(Some).to_vec()
    );
    assert_eq!(
        i.column(3).1,
        [105.0, 126.0, 147.0, 168.0, 189.0].map(Some).to_vec()
    );
    for index in [2, 3] {
        let (id, _) = i.column(index);
        assert!(
            i.state.tree().formula_letters(id).is_some(),
            "formula {index} intact"
        );
    }
    // One Undo restores the whole sheet, formulas and values.
    i.state.dispatch(Action::Undo);
    assert_eq!(**i.state.tree(), *before);
    let _ = (i.x, i.y);
}

/// A formula column has no source and is not a reason to keep the rows: the
/// sheet shrinks with the file and the formulas recompute (#433 point 6).
#[test]
fn refreshing_with_a_shorter_file_shrinks_the_sheet_and_recomputes_formulas() {
    let mut with = imported(true, true);
    let mut without = imported(false, false);
    for i in [&mut with, &mut without] {
        i.rewrite("5;50\n");
        let sheet = i.sheet;
        i.state.dispatch(Action::RefreshFromSource { sheet });
        assert!(!i.state.status().is_some_and(|s| s.is_error()));
    }
    assert_eq!(without.column(0).1, [Some(5.0)]);
    assert_eq!(with.column(0).1, [Some(5.0)]);
    assert_eq!(with.column(1).1, [Some(50.0)]);
    assert_eq!(with.column(2).1, [Some(100.0)]);
    assert_eq!(with.column(3).1, [Some(105.0)]);
    for index in [2, 3] {
        let (id, _) = with.column(index);
        assert!(
            with.state.tree().formula_letters(id).is_some(),
            "formula {index} intact"
        );
    }
    // One Undo restores the whole sheet.
    with.state.dispatch(Action::Undo);
    assert_eq!(with.column(0).1, [1.0, 2.0, 3.0].map(Some).to_vec());
    assert_eq!(with.column(2).1, [20.0, 40.0, 60.0].map(Some).to_vec());
    assert_eq!(with.column(3).1, [21.0, 42.0, 63.0].map(Some).to_vec());
    assert_eq!(with.state.row_count(with.sheet), 3);
}

/// A hand-typed column (no source) still blocks the shrinking.
#[test]
fn refreshing_with_a_shorter_file_keeps_the_rows_of_a_hand_typed_column() {
    let mut i = imported(true, true);
    let sheet = i.sheet;
    i.state.dispatch(Action::AddColumn { sheet });
    i.rewrite("5;50\n");
    i.state.dispatch(Action::RefreshFromSource { sheet });
    assert!(!i.state.status().is_some_and(|s| s.is_error()));
    assert_eq!(i.state.row_count(sheet), 3);
    assert_eq!(i.column(0).1, [Some(5.0), None, None]);
    assert_eq!(i.column(2).1, [Some(100.0), None, None]);
}

// -- column type of a formula column --

fn column_type_of(w: &World, column: NodeId) -> ColumnType {
    w.state
        .tree()
        .node(column)
        .unwrap()
        .as_column()
        .unwrap()
        .column_type()
}

#[test]
fn a_type_change_never_corrupts_a_formula_column() {
    let mut w = world();
    let sheet1 = w.sheet1;
    let e = w.formula_column(sheet1, "A*2");
    let values = w.values(e);
    for kind in [ColumnType::Mixed, ColumnType::DateTime, ColumnType::Numeric] {
        w.state.dispatch(Action::SetColumnType {
            column: e,
            column_type: kind,
        });
        w.state.dispatch(Action::SetColumnsType {
            columns: vec![e],
            column_type: kind,
        });
        assert_eq!(w.letters(e).as_deref(), Some("A*2"));
        assert_eq!(w.values(e), values, "{kind:?}");
    }
}

#[test]
fn a_type_change_on_a_formula_column_is_refused() {
    let mut w = world();
    let sheet1 = w.sheet1;
    let e = w.formula_column(sheet1, "A*2");
    let before = w.state.snapshot().history.undo_label.clone();
    w.state.dispatch(Action::SetColumnType {
        column: e,
        column_type: ColumnType::Numeric,
    });
    assert!(w.status().starts_with("[error]"), "{}", w.status());
    assert_eq!(column_type_of(&w, e), ColumnType::Mixed);
    assert_eq!(w.state.snapshot().history.undo_label, before);
}

/// #431, Q3: a selection holding a formula column and a plain one changes the
/// plain one only, with no error and one undo step; a selection of formula
/// columns only has nothing to change and says why.
#[test]
fn a_type_change_on_a_mixed_selection_applies_to_the_plain_columns_only() {
    let mut w = world();
    let sheet1 = w.sheet1;
    let f = w.formula_column(sheet1, "A*2");
    w.state.dispatch(Action::AddColumn { sheet: sheet1 });
    let plain = *w.columns(sheet1).last().unwrap();
    w.state.dispatch(Action::SetColumnsType {
        columns: vec![plain],
        column_type: ColumnType::Mixed,
    });
    assert_eq!(
        w.state.columns_with_changeable_type(&[f, plain]),
        vec![plain]
    );
    assert_eq!(w.state.column_type_lock(&[f, plain]), None);
    assert_eq!(
        w.state.column_type_lock(&[f]),
        Some(stokesforge_app_state::state::ColumnTypeLock::Formula)
    );
    assert!(w.state.columns_with_changeable_type(&[f]).is_empty());
    let g = w.formula_column(sheet1, "A*3");
    let before = w.state.tree().clone();
    w.state.dispatch(Action::SetColumnsType {
        columns: vec![plain, f, g],
        column_type: ColumnType::Numeric,
    });
    assert!(!w.status().starts_with("[error]"), "{}", w.status());
    assert_eq!(column_type_of(&w, plain), ColumnType::Numeric);
    assert_eq!(column_type_of(&w, f), ColumnType::Mixed);
    assert_eq!(column_type_of(&w, g), ColumnType::Mixed);
    w.state.dispatch(Action::Undo);
    assert_eq!(*w.state.tree(), before, "one undo step");
}

// -- stale snapshot during a long recalculation --

/// Slow tests: the name prefix `slow::` is THE marker (`cargo test -- --skip
/// slow::` leaves them out; `docs/tests.md`, "Slow tests").
mod slow {
    use super::*;

    /// Size of the heavy sheet. Measured in debug (2026-11-02): the recalculation
    /// after the paste lasts about 2.5 s (150 columns x 80 000 rows; the original 200 000 rows lasted 6.4 s), that is 12 times `RECALC_WAIT`.
    ///
    /// Mutation witness: reading every field's meta of a header paste from one
    /// snapshot taken before the write loop (`paste_header_text`) makes the first
    /// test fail at this size, and no fast test of this file notices it.
    const ROWS: u32 = 80_000;
    const FORMULAS: usize = 150;

    /// `ROWS` rows, `FORMULAS` formula columns reading `A`, plus the plain columns
    /// `P` and `Q` (the recalculation lasts several times the 200 ms wait).
    struct Heavy {
        _language: i18n::LanguageGuard,
        _session: Session,
        state: AppState,
        _dir: tempfile::TempDir,
        sheet: NodeId,
        a: NodeId,
        p: NodeId,
        q: NodeId,
        reader: NodeId,
    }

    fn heavy() -> Heavy {
        use stokesforge_core::{ColumnFormula, FormulaPart};
        let language = i18n::test_language(Language::English);
        let mut project = Project::new("H").unwrap();
        let root = project.root();
        let book = project.add_empty_workbook(root, Some("Book1")).unwrap();
        let sheet = project.add_worksheet(book, Some("Sheet1")).unwrap();
        let big: Vec<f64> = (0..ROWS).map(f64::from).collect();
        let a = data(&mut project, sheet, "A", &big);
        let p = data(&mut project, sheet, "P", &[1.0, 2.0, 3.0]);
        let q = data(&mut project, sheet, "Q", &[4.0, 5.0, 6.0]);
        let mut reader = a;
        for index in 0..FORMULAS {
            let id = data(&mut project, sheet, &format!("F{index}"), &[0.0]);
            let formula = ColumnFormula::from_parts(vec![
                FormulaPart::Column(a),
                FormulaPart::Text("*2+".into()),
                FormulaPart::Column(a),
                FormulaPart::Text("*3".into()),
            ]);
            project
                .set_column_meta(
                    id,
                    ColumnMeta {
                        formula: Some(formula),
                        ..Default::default()
                    },
                )
                .unwrap();
            reader = id;
        }
        let session = Session::spawn(project, None).unwrap();
        let dir = tempfile::tempdir().unwrap();
        let mut state = AppState::new_with_preferences(
            session.handle(),
            dir.path().join("h.stf").display().to_string(),
            stokesforge_app_state::preferences::Preferences::load_from(None),
        )
        .unwrap();
        // First pass over every formula (the spawned project holds none).
        state.dispatch(Action::AddFolder { parent: root });
        wait_recalc(&mut state);
        Heavy {
            _language: language,
            _session: session,
            state,
            _dir: dir,
            sheet,
            a,
            p,
            q,
            reader,
        }
    }

    fn wait_recalc(state: &mut AppState) {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(120);
        while state.recalc_work().is_some() {
            assert!(
                std::time::Instant::now() < deadline,
                "recalculation too long"
            );
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        state.poll_recalc();
    }

    #[test]
    fn a_header_paste_started_during_a_long_recalculation_writes_every_field() {
        let mut h = heavy();
        let (sheet, a, p, q) = (h.sheet, h.a, h.p, h.q);
        // Three header fields on Q, copied.
        for (field, text) in [
            (HeaderField::LongName, "long"),
            (HeaderField::Unit, "unit"),
            (HeaderField::Comment, "note"),
        ] {
            h.state.begin_header_edit(q, field);
            h.state.header_editing_mut().unwrap().buffer = text.into();
            h.state.commit_header_edit();
        }
        h.state.select_header_field(sheet, q, HeaderField::LongName);
        h.state
            .extend_header_field_selection(sheet, q, HeaderField::Comment);
        let text = h.state.copy_selection(sheet).unwrap();
        // A heavy recalculation: edit the big source column.
        h.state.select_cell(sheet, a, 0);
        h.state.dispatch(Action::Paste {
            sheet,
            text: "7".into(),
        });
        let running = h.state.recalc_work().is_some();
        assert!(
            running,
            "the test needs the long recalculation to be running"
        );
        // The gesture starts while the snapshot is stale.
        h.state.select_header_field(sheet, p, HeaderField::LongName);
        h.state.dispatch(Action::Paste { sheet, text });
        wait_recalc(&mut h.state);
        let meta = h.state.column_meta(p);
        assert_eq!(
            (
                meta.long_name.as_str(),
                meta.unit.as_str(),
                meta.comment.as_str()
            ),
            ("long", "unit", "note"),
            "recalc running at the start: {running}"
        );
        // The edit itself is not lost either.
        assert_eq!(
            h.state
                .tree()
                .node(a)
                .unwrap()
                .as_column()
                .unwrap()
                .values()
                .number_at(0),
            7.0
        );
    }

    #[test]
    fn deleting_a_column_with_readers_during_a_long_recalculation_is_consistent() {
        let mut h = heavy();
        let (sheet, a, p) = (h.sheet, h.a, h.p);
        h.state.select_cell(sheet, a, 1);
        h.state.dispatch(Action::Paste {
            sheet,
            text: "9".into(),
        });
        let running = h.state.recalc_work().is_some();
        assert!(
            running,
            "the test needs the long recalculation to be running"
        );
        h.state.select_column(sheet, p);
        h.state.dispatch(Action::DeleteSelectedCells { sheet });
        wait_recalc(&mut h.state);
        assert!(
            !h.state.tree().children(sheet).unwrap().contains(&p),
            "running: {running}"
        );
        let reader = h.reader;
        let node = h.state.tree().node(reader).unwrap().as_column().unwrap();
        assert_eq!(
            node.values().number_at(1),
            9.0 * 2.0 + 9.0 * 3.0,
            "reader saw the edit"
        );
        assert!(h.state.tree().formula_letters(reader).is_some());
    }
}

// -- a kept formula binds when its column appears (#433, Q3) --

/// A kept `A+F` binds when the column F is added: the status bar says which
/// column the formula now reads, the column computes, and undo and redo say
/// nothing about it (the session reports a binding once, an undo unbinds
/// without counting). Witness: no `tell_bound_formulas`, no message; a message
/// emitted from the tree instead of the count repeats on undo and redo.
#[test]
fn a_kept_formula_that_binds_says_which_column_it_now_reads_once() {
    let mut w = world();
    let e = new_target(&mut w);
    w.type_formula(e, "A+F");
    assert_eq!(w.letters(e).as_deref(), Some("A+F"), "kept: no column F");
    let missing = |w: &World| {
        w.state
            .formula_issue(e)
            .is_some_and(|issue| issue.contains("Column F does not exist"))
    };
    assert!(missing(&w), "framed: {:?}", w.state.formula_issue(e));
    let sheet1 = w.sheet1;
    w.state.dispatch(Action::AddColumn { sheet: sheet1 });
    assert_eq!(
        w.status(),
        "The formula of column E now reads A+F",
        "the message follows the gesture's own"
    );
    let level = w.state.status().unwrap().level;
    assert_eq!(
        level,
        stokesforge_app_state::state::StatusLevel::Warning,
        "a warning, kept until the next message"
    );
    assert!(!missing(&w), "bound: no longer framed for F");
    let told = |w: &World| {
        w.state
            .status_history()
            .filter(|message| message.text.contains("now reads"))
            .count()
    };
    // The count says it once: nothing new after a refresh.
    w.state.refresh();
    assert_eq!(told(&w), 1);
    // Undo unbinds and stays silent; so does redo, which only replays it.
    w.state.dispatch(Action::Undo);
    assert!(!w.status().contains("now reads"), "{}", w.status());
    assert!(missing(&w), "unbound again");
    w.state.dispatch(Action::Redo);
    assert!(!w.status().contains("now reads"), "{}", w.status());
    assert!(!missing(&w), "bound again");
    assert_eq!(told(&w), 1, "told once, whatever the undo and redo");
}

/// Several bindings in one gesture say how many, in a sentence of their own.
#[test]
fn several_bindings_in_one_gesture_are_counted() {
    let mut w = world();
    let e = new_target(&mut w);
    let f = new_target(&mut w);
    w.type_formula(e, "A+G");
    w.type_formula(f, "B+G");
    let sheet1 = w.sheet1;
    w.state.dispatch(Action::AddColumn { sheet: sheet1 });
    assert!(
        w.status().starts_with("2 formulas now read"),
        "{}",
        w.status()
    );
}

/// A kept formula typed over a column of **data** empties it (the formula is
/// the source of truth), and one Undo gives the data back, not only the old
/// meta (#433). Witness: without `values_before` in the `SetColumnMeta` delta
/// the column comes back empty.
#[test]
fn a_kept_formula_typed_over_data_is_undone_with_the_data() {
    let mut w = world();
    let sheet1 = w.sheet1;
    let d = w.columns(sheet1)[3];
    assert_eq!(
        w.values(d),
        [1000.0, 1001.0, 1002.0, 1003.0, 1004.0].map(Some)
    );
    w.type_formula(d, "A +");
    assert_eq!(w.letters(d).as_deref(), Some("A +"));
    assert!(w.values(d).iter().all(Option::is_none), "emptied");
    w.state.dispatch(Action::Undo);
    assert_eq!(w.letters(d), None, "no formula again");
    assert_eq!(
        w.values(d),
        [1000.0, 1001.0, 1002.0, 1003.0, 1004.0].map(Some)
    );
}

/// The warning of a comma rewrite lives only while the column holds the
/// rewritten formula (#433): red, undone, replaced or removed, it never shows
/// later. Witness: without the `formula_rewrite_holds` check a pending sentence
/// is handed out after the undo.
#[test]
fn a_comma_warning_dies_with_the_formula_it_was_about() {
    let mut w = world();
    let target = new_target(&mut w);
    // Red: A is the X, so `max(A,1,5)` has no X; the sentence stays pending.
    w.type_formula(target, "max(A,1,5)");
    assert!(w.state.formula_issue(target).is_some(), "red");
    w.state.dispatch(Action::Undo);
    assert_eq!(w.state.take_formula_warning(target), None, "undone");
    // Undone after being valid.
    w.type_formula(target, "max(B,1,5)");
    assert!(w.state.formula_rewrite_holds(target));
    w.state.dispatch(Action::Undo);
    assert!(!w.state.formula_rewrite_holds(target), "undone");
    assert_eq!(w.state.take_formula_warning(target), None);
    // Replaced, then removed.
    w.type_formula(target, "max(B,1,5)");
    w.type_formula(target, "B*2");
    assert!(!w.state.formula_rewrite_holds(target), "replaced");
    assert_eq!(w.state.take_formula_warning(target), None);
    w.type_formula(target, "max(B,1,5)");
    w.state.dispatch(Action::SetColumnFormula {
        column: target,
        formula: None,
    });
    assert!(!w.state.formula_rewrite_holds(target), "removed");
    assert_eq!(w.state.take_formula_warning(target), None);
    // The status sentence of the rewrite itself is not touched.
    w.type_formula(target, "max(B,1,5)");
    assert!(w.state.status().unwrap().text.contains("from 1 to 5"));
}

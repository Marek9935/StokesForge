//! Acceptance criteria of the UI shell, driven **without a window**.
//!
//! Everything that matters in this phase is checked here: `AppState` does not
//! depend on egui, so the whole loop (model → command → undo →
//! save → reload) runs in CI without a graphics server. The `eframe` layer
//! is only a keyboard/mouse → [`Action`] translation and a
//! painter; what it adds cannot be tested automatically and is
//! therefore not tested here (see criterion 13, which is a script).
//!
//! Each test starts **its own** `Session` on a demo project
//! built by the local helper [`demo_project`]: the tests share
//! no state and do not depend on the binary's demo project, which
//! can change without invalidating these assertions.
//!
//! # Grid scoped by workbook
//!
//! Since the grid is one tab per workbook (`Tab::Book`, rather than
//! the single global `Tab::Grid` from before this task), `AppState` no longer has
//! "the" selected sheet: [`AppState::visible_columns`] and
//! [`AppState::row_count`] take an explicit sheet `NodeId`, and
//! [`AppState::selected_sheet_in`] returns the sheet chosen *in a given
//! workbook*. The tests below therefore resolve the target workbook or sheet
//! by its name ([`book_named`], [`sheet_named`]) rather than
//! relying on an implicit selection.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use stokesforge_app_state::history_text::history_text;
use stokesforge_app_state::selection::Selection;
use stokesforge_app_state::state::{
    active_graph, drop_payload_action, drop_rows_action, file_drop_plan, Action, ActiveTabKind,
    AlertKind, AppState, ColumnDragPayload, GridZone, HeaderField, ImportDestination,
    MoveDirection, PlotAxis, PlotEdit, PlotSettingsTab, PlotTarget, RowAxis, StatusLevel,
    UnsavedChoice, UnsavedGesture, INFO_LIFETIME, STATUS_HISTORY_LEN,
};
use stokesforge_commands::{ColumnRef, HistoryLabel, MacroKind};
use stokesforge_core::{
    Cell, Column, ColumnFormula, ColumnMeta, ColumnRole, ColumnType, FormulaPart, NodeId, Project,
    Tree,
};
use stokesforge_import::{
    DecimalStyle, HeaderSkip, ImportRecipe, NameSource, RecipeStore, Separator,
};
use stokesforge_plot::{AxisScale, PlotType, RawVariant};
use stokesforge_session::{Session, SessionHandle};

/// Demo project of the tests: a workbook `Book1` with `Sheet1`
/// (columns `Time`, `Signal`, `Note`, the latter **shorter**) and
/// a second sheet `Sheet2` with a short column. The uneven
/// lengths are deliberate: it is the case the model allows and that the
/// grid must be able to paint.
fn demo_project() -> Project {
    let mut project = Project::new("Demo").expect("demo project");
    let root = project.root();
    let book = project
        .add_workbook(root, Some("Book1"))
        .expect("workbook Book1");
    let sheet1 = project
        .tree()
        .child_by_name(book, "Sheet1")
        .expect("Book1 readable")
        .expect("Sheet1 created with the workbook");
    project
        .add_column(
            sheet1,
            None,
            Some("Time"),
            Column::from_f64(&[0.0, 0.5, 1.0, 1.5, 2.0]),
        )
        .expect("column Time");
    project
        .add_column(
            sheet1,
            None,
            Some("Signal"),
            Column::from_f64(&[10.0, f64::NAN, 30.0, 40.0, 50.0]),
        )
        .expect("column Signal");
    project
        .add_column(
            sheet1,
            None,
            Some("Note"),
            Column::from_text(&[Some("a".to_string()), None, Some(String::new())]),
        )
        .expect("column Note");
    let sheet2 = project
        .add_worksheet(book, Some("Sheet2"))
        .expect("worksheet Sheet2");
    project
        .add_column(sheet2, None, Some("Short"), Column::from_f64(&[7.0, 8.0]))
        .expect("column Court");
    project
}

/// Minimal project holding only **one** sheet: used to produce a
/// file whose node identifiers do not overlap those of
/// [`demo_project`] (criterion 8).
fn single_sheet_project() -> Project {
    let mut project = Project::new("Other").expect("project Other");
    let root = project.root();
    let book = project
        .add_workbook(root, Some("Book1"))
        .expect("workbook Book1");
    let sheet = project
        .tree()
        .child_by_name(book, "Sheet1")
        .expect("Book1 readable")
        .expect("Sheet1 created with the workbook");
    project
        .add_column(sheet, None, Some("Unique"), Column::from_f64(&[1.0, 2.0]))
        .expect("column Unique");
    project
}

/// Starts a session on `project` and returns the associated application state, as well
/// as the session (keep it alive: dropping it ends the actor).
fn start(project: Project, path: &Path) -> (Session, AppState) {
    let session = Session::spawn(project, None).expect("session thread");
    let state = AppState::new_with_preferences(
        session.handle(),
        path.display().to_string(),
        stokesforge_app_state::preferences::Preferences::load_from(None),
    )
    .expect("initial application state");
    (session, state)
}

/// Session on the demo project, working file in `dir`.
fn start_demo(dir: &Path) -> (Session, AppState) {
    start(demo_project(), &dir.join("projet.stf"))
}

#[test]
fn an_empty_graph_is_undoable_redoable_and_saved() {
    let dir = tempfile::tempdir().unwrap();
    let (session, mut state) = start_demo(dir.path());
    let before = state.tree().len();
    state.dispatch(Action::AddGraph);
    let graph = *state
        .tree()
        .node(state.tree().root())
        .unwrap()
        .children()
        .last()
        .unwrap();
    let spec = state.plot_spec(graph).unwrap();
    assert!(spec.title.is_empty(), "#103: an empty graph has no title");
    assert_eq!(spec.panels.len(), 1);
    assert!(spec.panels.iter().all(|panel| panel.series.is_empty()));
    state.dispatch(Action::Undo);
    assert_eq!(state.tree().len(), before);
    state.dispatch(Action::Redo);
    assert!(state.tree().node(graph).is_some());
    state.dispatch(Action::Save);
    state.dispatch(Action::Load);
    assert!(state.plot_spec(graph).unwrap().title.is_empty());
    assert!(state.plot_spec(graph).unwrap().panels[0].series.is_empty());
    session.shutdown().unwrap();
}

#[test]
fn a_new_project_waits_for_confirmation_and_cancelling_preserves_the_work() {
    let dir = tempfile::tempdir().unwrap();
    let (session, mut state) = start_demo(dir.path());
    let before = state.tree().len();
    state.dispatch(Action::NewProject {
        pending_edits: false,
    });
    assert_eq!(state.unsaved_pending(), Some(&UnsavedGesture::NewProject));
    assert!(state.blocking_dialog());
    assert_eq!(state.tree().len(), before);
    state.dispatch(Action::ConfirmUnsaved(UnsavedChoice::Cancel));
    assert!(state.unsaved_pending().is_none());
    assert_eq!(state.tree().len(), before);
    state.dispatch(Action::NewProject {
        pending_edits: false,
    });
    state.dispatch(Action::ConfirmUnsaved(UnsavedChoice::Discard));
    assert_eq!(state.tree().len(), 1);
    assert!(state.path().is_empty());
    assert!(!state.can_undo());
    // No confirmation for a second, still empty project.
    state.dispatch(Action::NewProject {
        pending_edits: false,
    });
    assert!(state.unsaved_pending().is_none());
    state.dispatch(Action::AddWorkbook);
    state.dispatch(Action::NewProject {
        pending_edits: false,
    });
    assert!(state.unsaved_pending().is_some());
    session.shutdown().unwrap();
}

#[test]
fn a_new_project_after_saving_and_after_a_failure() {
    let dir = tempfile::tempdir().unwrap();
    let (session, mut state) = start_demo(dir.path());
    state.dispatch(Action::Save);
    state.dispatch(Action::NewProject {
        pending_edits: true,
    });
    assert!(state.unsaved_pending().is_some());
    state.dispatch(Action::ConfirmUnsaved(UnsavedChoice::Cancel));
    state.dispatch(Action::NewProject {
        pending_edits: false,
    });
    assert!(state.unsaved_pending().is_none());
    assert_eq!(state.tree().len(), 1);
    state.dispatch(Action::AddGraph);
    state.set_path(dir.path().display().to_string()); // a destination that is a folder
    state.dispatch(Action::Save);
    // S-M8 (#229): a failed save is a serious failure, reported by a
    // blocking dialog that offers Save As….
    let alert = state.alert().expect("serious failure");
    assert_eq!(alert.kind, AlertKind::SaveFailed);
    assert!(!alert.message.is_empty());
    assert!(state.blocking_dialog());
    state.dispatch(Action::DismissAlert);
    assert!(state.alert().is_none());
    state.dispatch(Action::NewProject {
        pending_edits: false,
    });
    assert!(state.unsaved_pending().is_some());
    session.shutdown().unwrap();
}

/// S-F4 (#229): the Save answer saves, **then** continues the gesture; a
/// failed save abandons it (the work stays) and reports it (S-M8).
#[test]
fn the_save_answer_saves_then_continues_the_gesture() {
    let dir = tempfile::tempdir().unwrap();
    let (session, mut state) = start_demo(dir.path());
    let file = dir.path().join("work.stf");
    state.set_path(file.display().to_string());
    state.dispatch(Action::AddGraph);
    state.dispatch(Action::NewProject {
        pending_edits: false,
    });
    state.dispatch(Action::ConfirmUnsaved(UnsavedChoice::Save));
    assert!(file.is_file(), "saved");
    assert_eq!(state.tree().len(), 1, "then the new project");
    assert!(state.unsaved_pending().is_none());

    // Failure: the pending gesture is abandoned, the work stays.
    state.dispatch(Action::AddWorkbook);
    let len = state.tree().len();
    state.set_path(dir.path().display().to_string());
    state.dispatch(Action::NewProject {
        pending_edits: false,
    });
    state.dispatch(Action::ConfirmUnsaved(UnsavedChoice::Save));
    assert_eq!(state.tree().len(), len, "the work stays");
    assert_eq!(
        state.alert().map(|alert| alert.kind),
        Some(AlertKind::SaveFailed)
    );
    assert!(state.unsaved_pending().is_none());
    // A later successful save does not replay the abandoned gesture.
    state.dispatch(Action::DismissAlert);
    state.set_path(file.display().to_string());
    state.dispatch(Action::Save);
    assert_eq!(state.tree().len(), len);

    // Without a file: Save records the gesture, which the save will continue
    // (the window asks where to write, then pushes Action::Save); Cancel
    // (picker closed without a choice) forgets it.
    state.dispatch(Action::AddGraph);
    state.set_path(String::new());
    state.dispatch(Action::Quit {
        pending_edits: false,
    });
    state.dispatch(Action::ConfirmUnsaved(UnsavedChoice::Save));
    assert!(!state.take_quit_ready(), "nothing saved yet");
    state.dispatch(Action::ConfirmUnsaved(UnsavedChoice::Cancel));
    state.set_path(file.display().to_string());
    state.dispatch(Action::Save);
    assert!(!state.take_quit_ready(), "gesture forgotten");
    state.dispatch(Action::AddGraph);
    state.set_path(String::new());
    state.dispatch(Action::Quit {
        pending_edits: false,
    });
    state.dispatch(Action::ConfirmUnsaved(UnsavedChoice::Save));
    state.set_path(file.display().to_string());
    state.dispatch(Action::Save);
    assert!(state.take_quit_ready(), "saved, then Quit");
    session.shutdown().unwrap();
}

/// `STOKESFORGE_DEMO=1`: the forced demo closes without a question, even
/// when modified; New and Open… still ask. Witness: without the setting,
/// the same close asks the question.
#[test]
fn the_forced_demo_closes_without_asking_to_save() {
    let dir = tempfile::tempdir().unwrap();
    let (session, mut state) = start_demo(dir.path());
    state.dispatch(Action::AddGraph);
    state.dispatch(Action::Quit {
        pending_edits: false,
    });
    assert_eq!(
        state.unsaved_pending(),
        Some(&UnsavedGesture::Quit),
        "witness: without the setting, the question is asked"
    );
    state.dispatch(Action::ConfirmUnsaved(UnsavedChoice::Cancel));

    state.set_quit_without_prompt(true);
    state.dispatch(Action::Quit {
        pending_edits: true,
    });
    assert!(state.unsaved_pending().is_none());
    assert!(state.take_quit_ready(), "direct close");

    state.dispatch(Action::NewProject {
        pending_edits: false,
    });
    assert_eq!(
        state.unsaved_pending(),
        Some(&UnsavedGesture::NewProject),
        "New project still asks"
    );
    session.shutdown().unwrap();
}

/// S-F3 (#229): Quit and Open… ask only if there is work to
/// lose; Discard continues. An unreadable project keeps the current project **and**
/// path (S-M8).
#[test]
fn quit_and_open_ask_only_if_work_would_be_lost() {
    let dir = tempfile::tempdir().unwrap();
    let (session, mut state) = start_demo(dir.path());
    let file = dir.path().join("a.stf");
    state.set_path(file.display().to_string());
    state.dispatch(Action::Save);
    state.dispatch(Action::Quit {
        pending_edits: false,
    });
    assert!(state.unsaved_pending().is_none());
    assert!(state.take_quit_ready(), "nothing to lose: direct Quit");
    assert!(!state.take_quit_ready(), "only once");

    state.dispatch(Action::Quit {
        pending_edits: true,
    });
    assert_eq!(state.unsaved_pending(), Some(&UnsavedGesture::Quit));
    assert!(!state.take_quit_ready());
    state.dispatch(Action::ConfirmUnsaved(UnsavedChoice::Discard));
    assert!(state.take_quit_ready());

    // Open… of an unreadable file, with no work to lose.
    let len = state.tree().len();
    let bad = dir.path().join("unreadable.stf");
    std::fs::write(&bad, "not a project").unwrap();
    state.dispatch(Action::OpenProject {
        path: bad.display().to_string(),
        pending_edits: false,
    });
    assert_eq!(
        state.alert().map(|alert| alert.kind),
        Some(AlertKind::OpenFailed)
    );
    assert_eq!(state.path(), file.display().to_string(), "path kept");
    assert_eq!(state.tree().len(), len);
    state.dispatch(Action::DismissAlert);

    // Open… with work: a question, then Discard opens.
    let other = dir.path().join("b.stf");
    state.set_path(other.display().to_string());
    state.dispatch(Action::Save);
    state.set_path(file.display().to_string());
    state.dispatch(Action::AddGraph);
    state.dispatch(Action::OpenProject {
        path: other.display().to_string(),
        pending_edits: false,
    });
    assert!(matches!(
        state.unsaved_pending(),
        Some(UnsavedGesture::Open(path)) if *path == other.display().to_string()
    ));
    assert_eq!(state.path(), file.display().to_string(), "not opened yet");
    state.dispatch(Action::ConfirmUnsaved(UnsavedChoice::Discard));
    assert_eq!(state.path(), other.display().to_string());
    assert_eq!(state.tree().len(), len);
    session.shutdown().unwrap();
}

/// Identifier of the column `name` in `sheet`.
fn column_id(state: &AppState, sheet: NodeId, name: &str) -> NodeId {
    state
        .visible_columns(sheet)
        .into_iter()
        .find(|(_, column_name, _)| column_name == name)
        .map(|(id, _, _)| id)
        .unwrap_or_else(|| panic!("column {name} visible in {sheet}"))
}

/// Raw model cell (beyond what `cell_text` lets you see).
fn cell(state: &AppState, column: NodeId, row: usize) -> Cell {
    state
        .tree()
        .get(column)
        .expect("column present")
        .as_column()
        .expect("column node")
        .get(row)
}

/// Type of a column in the current tree.
fn column_type(state: &AppState, column: NodeId) -> ColumnType {
    state
        .tree()
        .get(column)
        .expect("column present")
        .as_column()
        .expect("column node")
        .column_type()
}

/// Sheet named `name` in the current tree.
fn sheet_named(tree: &Tree, name: &str) -> NodeId {
    tree.walk_all()
        .into_iter()
        .find(|id| {
            tree.node(*id)
                .is_some_and(|node| node.kind().is_worksheet() && node.name() == name)
        })
        .unwrap_or_else(|| panic!("sheet {name} present"))
}

/// Workbook named `name` in the current tree. Counterpart of [`sheet_named`],
/// needed since the sheet selection is scoped by workbook
/// ([`AppState::selected_sheet_in`]).
fn book_named(tree: &Tree, name: &str) -> NodeId {
    tree.walk_all()
        .into_iter()
        .find(|id| {
            tree.node(*id).is_some_and(|node| {
                matches!(
                    node.kind(),
                    stokesforge_core::node::NodeKind::Workbook { .. }
                ) && node.name() == name
            })
        })
        .unwrap_or_else(|| panic!("workbook {name} present"))
}

/// Session handle unused outside construction: silences the lint on the
/// tests that do not need it.
#[allow(dead_code)]
fn unused(_: &SessionHandle) {}

/// Protects the tests that isolate the import-recipe config folder
/// (see [`isolate_recipe_store`]) from other tests of this same binary
/// that would do the same in parallel: `std::env::set_var` is global
/// *process* state, never scoped to a thread — `cargo test` runs
/// the tests of this file on several threads by default.
static IMPORT_RECIPES_ENV_LOCK: Mutex<()> = Mutex::new(());

/// Redirects the config folder to a temporary directory specific to the
/// call, for the lifetime of the returned value: `stokesforge_import::
/// RecipeStore::load`/`save` (hence `AppState::open_import`/`save_import_recipe`/
/// `delete_import_recipe`) go through `stokesforge_import::config_dir::
/// config_dir`, which reads `STOKESFORGE_CONFIG_DIR` (#332). `XDG_CONFIG_HOME`
/// is set too: it is the only one `directories` honors, and on
/// Linux only. Without `STOKESFORGE_CONFIG_DIR`, these methods read and
/// wrote the machine's real profile on Windows — not hermetic,
/// and racing between tests ("Access is denied").
///
/// Keeps the lock and the temporary directory alive until the
/// returned value is dropped (end of the test, panic included:
/// `Drop` restores both variables to their earlier value).
fn isolate_recipe_store() -> RecipeStoreIsolation {
    let lock = IMPORT_RECIPES_ENV_LOCK
        .lock()
        .unwrap_or_else(|err| err.into_inner());
    let dir = tempfile::tempdir().expect("temporary directory for the import recipes");
    let names = [
        stokesforge_import::config_dir::CONFIG_DIR_VARIABLE,
        "XDG_CONFIG_HOME",
    ];
    let previous = names.map(|name| (name, std::env::var_os(name)));
    for name in names {
        std::env::set_var(name, dir.path());
    }
    RecipeStoreIsolation {
        previous,
        _lock: lock,
        dir,
    }
}

/// [`isolate_recipe_store`], then saves the test templates of
/// `test/import/test_recipes.json` in the redirected folder, like templates
/// the user saved (no template exists in the program, #378). Only the tests
/// of automatic choice and of template selection need them.
fn with_test_recipes() -> RecipeStoreIsolation {
    let isolation = isolate_recipe_store();
    std::fs::copy(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../test/import/test_recipes.json"),
        isolation.dir.path().join("import_recipes.json"),
    )
    .expect("copy of the test templates");
    isolation
}

/// What [`isolate_recipe_store`] returns: the lock, the temporary folder and
/// the earlier values of the variables, restored on drop.
struct RecipeStoreIsolation {
    previous: [(&'static str, Option<std::ffi::OsString>); 2],
    _lock: std::sync::MutexGuard<'static, ()>,
    dir: tempfile::TempDir,
}

impl RecipeStoreIsolation {
    /// The temporary config folder.
    fn path(&self) -> &std::path::Path {
        self.dir.path()
    }
}

impl Drop for RecipeStoreIsolation {
    fn drop(&mut self) {
        for (name, value) in self.previous.iter_mut() {
            match value.take() {
                Some(value) => std::env::set_var(name, value),
                None => std::env::remove_var(name),
            }
        }
    }
}

/// #332: under the isolation, the config folder is the temporary folder
/// (equal `Path`, not a string), and saving an import template there
/// writes its file. On Windows, `XDG_CONFIG_HOME` alone is not enough:
/// `directories` ignores it, and the template went to the real profile.
#[test]
fn the_isolation_redirects_the_config_folder_and_saving_writes_there() {
    let isolation = isolate_recipe_store();
    assert_eq!(
        stokesforge_import::config_dir::config_dir().as_deref(),
        Some(isolation.path()),
        "the config folder is the temporary folder"
    );
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    state.open_import(None::<NodeId>);
    state.import_state_mut().unwrap().save_as_name = "Witness".to_string();
    state.save_import_recipe();
    assert!(!state.status().unwrap().is_error());
    let file = isolation.path().join("import_recipes.json");
    assert!(file.is_file(), "the template is written to {file:?}");
    assert!(std::fs::read_to_string(&file).unwrap().contains("Witness"));
}

// -- 1. Initial state --

#[test]
fn a_books_grid_defaults_to_its_first_sheet_and_describes_it() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, state) = start_demo(dir.path());

    let book1 = book_named(state.tree(), "Book1");
    let sheet = state.selected_sheet_in(book1).expect("a default sheet");
    assert_eq!(sheet, sheet_named(state.tree(), "Sheet1"));

    let columns = state.visible_columns(sheet);
    let described: Vec<(&str, ColumnType)> = columns
        .iter()
        .map(|(_, name, kind)| (name.as_str(), *kind))
        .collect();
    assert_eq!(
        described,
        vec![
            ("Time", ColumnType::Numeric),
            ("Signal", ColumnType::Numeric),
            ("Note", ColumnType::Mixed),
        ]
    );

    // Maximum of the lengths (5, 5, 3), not the length of the first column.
    assert_eq!(state.row_count(sheet), 5);

    assert!(!state.can_undo());
    assert!(!state.can_redo());
    assert_eq!(state.undo_label(), None);
    assert_eq!(state.redo_label(), None);
}

// -- 2. Entering a cell --

/// #222: the grid paints 6 significant digits, but editing, copying
/// and the input comparison keep the full value.
#[test]
fn the_grid_paints_six_digits_but_edits_and_copies_the_exact_value() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let time_col = column_id(&state, sheet1, "Time");
    state.dispatch(Action::CommitCell {
        column: time_col,
        row: 1,
        text: "12.345678901".to_string(),
    });

    assert_eq!(state.cell_display(time_col, 1), "12.3457");
    assert_eq!(state.cell_text(time_col, 1), "12.345678901");
    state.begin_edit(time_col, 1);
    assert_eq!(
        state.editing().map(|edit| edit.buffer.clone()),
        Some("12.345678901".to_string()),
        "the edit buffer holds the full value"
    );
}

/// #218: a Date/Time column is displayed, edited and copied in ISO 8601;
/// `10/10/2026 14:30` (day first) is read as a timestamp; a bare number stays
/// a timestamp; a text that is not a date promotes the column to Mixed.
#[test]
fn a_datetime_cell_shows_iso_and_reads_day_first_dates() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let time_col = column_id(&state, sheet1, "Time");
    state.dispatch(Action::SetColumnType {
        column: time_col,
        column_type: ColumnType::DateTime,
    });
    state.dispatch(Action::CommitCell {
        column: time_col,
        row: 0,
        text: "10/10/2026 14:30".to_string(),
    });
    assert_eq!(state.cell_display(time_col, 0), "2026-10-10 14:30");
    assert_eq!(state.cell_text(time_col, 0), "2026-10-10 14:30");
    let iso = match cell(&state, time_col, 0) {
        Cell::Float(value) => value,
        other => panic!("timestamp expected, got {other:?}"),
    };

    // Day first: 10/11/2026 is 10 November.
    state.dispatch(Action::CommitCell {
        column: time_col,
        row: 1,
        text: "10/11/2026".to_string(),
    });
    assert_eq!(state.cell_text(time_col, 1), "2026-11-10 00:00");
    // The ISO form reads back identically.
    state.dispatch(Action::CommitCell {
        column: time_col,
        row: 2,
        text: "2026-10-10 14:30".to_string(),
    });
    assert_eq!(cell(&state, time_col, 2), Cell::Float(iso));
    // A bare number stays a POSIX timestamp.
    state.dispatch(Action::CommitCell {
        column: time_col,
        row: 3,
        text: "86400".to_string(),
    });
    assert_eq!(state.cell_text(time_col, 3), "1970-01-02 00:00");
    // Month first (13th month): not a date, the column becomes Mixed.
    state.dispatch(Action::CommitCell {
        column: time_col,
        row: 3,
        text: "10/13/2026".to_string(),
    });
    assert_eq!(column_type(&state, time_col), ColumnType::Mixed);
}

/// #223: the comma is read as a decimal point in a Numeric cell;
/// the interface always writes back with the point. "1,000" is read as 1 (S-R6).
#[test]
fn a_decimal_comma_is_read_in_a_numeric_cell_and_shown_with_a_point() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let time_col = column_id(&state, sheet1, "Time");

    state.dispatch(Action::CommitCell {
        column: time_col,
        row: 1,
        text: "42,5".to_string(),
    });
    assert_eq!(cell(&state, time_col, 1), Cell::Float(42.5));
    assert_eq!(state.cell_text(time_col, 1), "42.5");

    state.dispatch(Action::CommitCell {
        column: time_col,
        row: 2,
        text: "1,000".to_string(),
    });
    assert_eq!(cell(&state, time_col, 2), Cell::Float(1.0));

    // Witness: an ambiguous comma is not a number, the column becomes mixed
    // rather than losing the text.
    state.dispatch(Action::CommitCell {
        column: time_col,
        row: 3,
        text: "1,2,3".to_string(),
    });
    assert_eq!(state.cell_text(time_col, 3), "1,2,3");
}

#[test]
fn committing_a_numeric_cell_writes_it_and_names_the_undo() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let time_col = column_id(&state, sheet1, "Time");
    let before = state.snapshot().revision;

    state.dispatch(Action::CommitCell {
        column: time_col,
        row: 1,
        text: "42.5".to_string(),
    });

    assert_eq!(cell(&state, time_col, 1), Cell::Float(42.5));
    assert_eq!(state.cell_text(time_col, 1), "42.5");
    assert!(state.can_undo());
    // Column A, not "Time": the label designates the column as the
    // grid displays it (its position letter, `ColumnRef`), never by its
    // node name, which appears nowhere on screen. `Time` remains the node name
    // of this column (it is how `column_id` finds it
    // here), it simply no longer belongs in a label meant for the
    // user.
    assert_eq!(
        state.undo_label(),
        Some(undo_entry(&HistoryLabel::SetCells {
            column: Some(ColumnRef(0))
        }))
    );
    assert!(state.snapshot().revision > before);
}

// -- 3. Non-numeric input in a numeric column --

/// Typing text into a numeric column **is no longer an error**: the
/// column becomes [`ColumnType::Mixed`] and the cell holds the text, the
/// others keeping their numbers (`Command::SetCellsPromoting`). Before
/// mixed-type columns, the input was refused and the whole column had to be
/// converted to text — making everything else unplottable — to
/// note an "n/a" on a single row.
#[test]
fn non_numeric_input_promotes_the_column_to_mixed() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let time_col = column_id(&state, sheet1, "Time");
    let before = state.snapshot().revision;
    let voisin = cell(&state, time_col, 0);

    state.dispatch(Action::CommitCell {
        column: time_col,
        row: 1,
        text: "n/a".to_string(),
    });

    assert!(
        state.status().is_none_or(|status| !status.is_error()),
        "a non-numeric input is no longer an error"
    );
    assert_eq!(column_type(&state, time_col), ColumnType::Mixed);
    assert_eq!(state.cell_text(time_col, 1), "n/a");
    assert_eq!(
        cell(&state, time_col, 0),
        voisin,
        "the other rows keep their number"
    );
    assert!(state.snapshot().revision > before);

    // A single history entry for the conversion **and** the write:
    // undoing brings back both the earlier type and value.
    state.dispatch(Action::Undo);
    assert_eq!(column_type(&state, time_col), ColumnType::Numeric);
    assert_eq!(cell(&state, time_col, 1), Cell::Float(0.5), "value intact");

    // And the second tolerant input, on an already mixed column, no longer
    // changes the type: it simply writes a text cell.
    state.dispatch(Action::Redo);
    state.dispatch(Action::CommitCell {
        column: time_col,
        row: 2,
        text: "hs".to_string(),
    });
    assert_eq!(column_type(&state, time_col), ColumnType::Mixed);
    assert_eq!(state.cell_text(time_col, 2), "hs");
    // And a number typed into a column that became mixed stays a number.
    state.dispatch(Action::CommitCell {
        column: time_col,
        row: 3,
        text: "7.25".to_string(),
    });
    assert_eq!(cell(&state, time_col, 3), Cell::Float(7.25));
}

// -- 4. Missing value --

#[test]
fn blank_text_writes_a_missing_numeric_value() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let time_col = column_id(&state, sheet1, "Time");

    state.dispatch(Action::CommitCell {
        column: time_col,
        row: 2,
        text: "   ".to_string(),
    });

    match cell(&state, time_col, 2) {
        Cell::Float(value) => assert!(value.is_nan(), "missing value = NaN"),
        other => panic!("numeric cell expected, got {other:?}"),
    }
    assert_eq!(state.cell_text(time_col, 2), "");
    assert!(
        state.status().is_none_or(|status| !status.is_error()),
        "writing a missing value is not an error"
    );
}

#[test]
fn empty_text_writes_a_missing_text_value_and_undo_restores_it() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let note = column_id(&state, sheet1, "Note");

    state.dispatch(Action::CommitCell {
        column: note,
        row: 0,
        text: "a".to_string(),
    });
    assert_eq!(cell(&state, note, 0), Cell::Text(Some("a".to_string())));

    state.dispatch(Action::CommitCell {
        column: note,
        row: 0,
        text: String::new(),
    });
    assert_eq!(cell(&state, note, 0), Cell::Text(None), "missing, not \"\"");
    // A9 (option 2, `guide-de-correction-robustesse.md` §F1/§F8): an absent
    // value is no longer displayed as an empty string (which would represent a
    // *present* empty string) but per the convention of `cell_text` — a
    // non-breaking space, compared here by property rather than by its literal
    // value so as not to duplicate the constant outside `stokesforge-app`.
    assert_ne!(
        state.cell_text(note, 0),
        "",
        "an absent value is no longer displayed as a present empty string"
    );

    state.dispatch(Action::Undo);
    assert_eq!(cell(&state, note, 0), Cell::Text(Some("a".to_string())));
    assert_eq!(state.cell_text(note, 0), "a");
}

// -- 5. Undo / redo --

#[test]
fn undo_and_redo_walk_the_history_both_ways() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let time_col = column_id(&state, sheet1, "Time");

    state.dispatch(Action::CommitCell {
        column: time_col,
        row: 1,
        text: "42.5".to_string(),
    });

    state.dispatch(Action::Undo);
    assert_eq!(
        cell(&state, time_col, 1),
        Cell::Float(0.5),
        "original value"
    );
    assert!(state.can_redo());
    assert_eq!(
        state.redo_label(),
        Some(redo_entry(&HistoryLabel::SetCells {
            column: Some(ColumnRef(0))
        }))
    );

    state.dispatch(Action::Redo);
    assert_eq!(cell(&state, time_col, 1), Cell::Float(42.5));
}

// -- 6. Undo on empty --

#[test]
fn undo_on_an_empty_history_is_a_notice_not_an_error() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let before = state.snapshot().revision;

    state.dispatch(Action::Undo);

    let status = state.status().expect("a message");
    assert_eq!(
        status.level,
        StatusLevel::Warning,
        "\"nothing to undo\" is the counterpart of the greyed-out button: a warning, not an error"
    );
    assert_eq!(state.snapshot().revision, before);

    state.dispatch(Action::Redo);
    let status = state.status().expect("a message");
    assert!(!status.is_error());
    assert_eq!(state.snapshot().revision, before);
}

// -- 7. The full loop — phase exit criterion --

#[test]
fn the_full_loop_returns_the_saved_tree_bit_for_bit() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let time_col = column_id(&state, sheet1, "Time");

    state.dispatch(Action::CommitCell {
        column: time_col,
        row: 1,
        text: "42.5".to_string(),
    });
    let saved_tree = std::sync::Arc::clone(state.tree());

    state.dispatch(Action::Save);
    assert!(
        state.status().is_some_and(|status| !status.is_error()),
        "save succeeded: {:?}",
        state.status()
    );

    state.dispatch(Action::CommitCell {
        column: time_col,
        row: 1,
        text: "99.0".to_string(),
    });
    assert_eq!(cell(&state, time_col, 1), Cell::Float(99.0));

    state.dispatch(Action::Load);
    assert!(
        state.status().is_some_and(|status| !status.is_error()),
        "load succeeded: {:?}",
        state.status()
    );

    assert_eq!(
        **state.tree(),
        *saved_tree,
        "the reloaded tree is identical to the one after the first input"
    );
    assert!(!state.can_undo(), "loading clears the history");
    assert!(!state.can_redo());

    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let time_col = column_id(&state, sheet1, "Time");
    assert_eq!(state.cell_text(time_col, 1), "42.5");
}

// -- 8. Reload and stale selection --

#[test]
fn loading_a_foreign_file_leaves_no_stale_per_book_selection() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let mine = dir.path().join("mine.stf");
    let other = dir.path().join("other.stf");

    // The foreign file is written by a separate session.
    {
        let (other_session, mut other_state) = start(single_sheet_project(), &other);
        other_state.dispatch(Action::Save);
        assert!(other_state.status().is_some_and(|s| !s.is_error()));
        other_session.shutdown().expect("session shutdown");
    }

    let (_session, mut state) = start_demo(&mine);
    let book1 = book_named(state.tree(), "Book1");
    let sheet2 = sheet_named(state.tree(), "Sheet2");
    state.dispatch(Action::SelectSheet(sheet2));
    assert_eq!(state.selected_sheet_in(book1), Some(sheet2));
    state.dispatch(Action::Save);

    state.set_path(other.display().to_string());
    state.dispatch(Action::Load);

    assert!(
        !state.tree().contains(sheet2),
        "the foreign file does not carry the identifier of Sheet2 \
         (otherwise the criterion tests nothing) — `book1`, created earlier in the \
         two projects, may on the other hand coincide: that is not what this \
         criterion checks"
    );

    // Self-healing: a workbook of the new tree, never explicitly
    // selected, falls back on its own to its first sheet — see the doc
    // of `AppState::selected_sheet_in`.
    let book = book_named(state.tree(), "Book1");
    let sheet = state
        .selected_sheet_in(book)
        .expect("a default sheet in the new file's workbook");
    assert!(state
        .tree()
        .node(sheet)
        .is_some_and(|node| node.kind().is_worksheet()));
    let names: Vec<String> = state
        .visible_columns(sheet)
        .into_iter()
        .map(|(_, name, _)| name)
        .collect();
    assert_eq!(names, vec!["Unique".to_string()]);
}

// -- 9. Invalid path --

#[test]
fn a_failing_load_leaves_the_model_and_history_untouched() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let time_col = column_id(&state, sheet1, "Time");
    state.dispatch(Action::CommitCell {
        column: time_col,
        row: 0,
        text: "3.0".to_string(),
    });
    let revision = state.snapshot().revision;
    let tree = std::sync::Arc::clone(state.tree());

    state.set_path(dir.path().join("missing.stf").display().to_string());
    state.dispatch(Action::Load);

    // S-M8 (#229): an unreadable project is a serious failure, reported by a dialog.
    assert_eq!(
        state.alert().map(|alert| alert.kind),
        Some(AlertKind::OpenFailed)
    );
    assert_eq!(state.snapshot().revision, revision);
    assert_eq!(**state.tree(), *tree);
    assert!(state.can_undo(), "the history is preserved");
}

/// A state whose opening cap is set by the preferences file
/// of `dir` (`max_file_mb` in MB, A2 #316).
fn start_with_max_file_mb(
    project: Project,
    path: &Path,
    dir: &Path,
    mb: u32,
) -> (Session, AppState) {
    let preferences_file = dir.join("preferences.json");
    std::fs::write(&preferences_file, format!(r#"{{"max_file_mb":{mb}}}"#)).unwrap();
    let session = Session::spawn(project, None).expect("session thread");
    let state = AppState::new_with_preferences(
        session.handle(),
        path.display().to_string(),
        stokesforge_app_state::preferences::Preferences::load_from(Some(preferences_file)),
    )
    .expect("initial application state");
    (session, state)
}

/// A2 (#316): a file above the opening cap is refused by a
/// dialog, the current project and path are kept.
#[test]
fn opening_a_file_over_the_limit_is_refused_and_keeps_the_project_and_path() {
    stokesforge_app_state::i18n::set_language(stokesforge_app_state::i18n::Language::English);
    let dir = tempfile::tempdir().expect("temporary directory");
    let current = dir.path().join("project.stf");
    let (_session, mut state) = start_with_max_file_mb(demo_project(), &current, dir.path(), 16);
    state.dispatch(Action::Save);
    let len = state.tree().len();
    // 16 MB + 1 byte: the content need not be a project, the size is enough.
    let big = dir.path().join("big.stf");
    std::fs::write(&big, vec![0u8; (16 << 20) + 1]).unwrap();
    state.dispatch(Action::OpenProject {
        path: big.display().to_string(),
        pending_edits: false,
    });
    let alert = state.alert().expect("a failure dialog");
    assert_eq!(alert.kind, AlertKind::OpenFailed);
    assert!(
        alert.message.contains("too large") && alert.message.contains("Preferences"),
        "unexpected message: {}",
        alert.message
    );
    assert_eq!(state.path(), current.display().to_string(), "path kept");
    assert_eq!(state.tree().len(), len, "project kept");
}

/// A2 (#316): a save above the cap succeeds, but warns about it
/// (instead of "Saved"), saying how to raise the limit.
#[test]
fn saving_over_the_opening_limit_succeeds_with_a_warning() {
    stokesforge_app_state::i18n::set_language(stokesforge_app_state::i18n::Language::English);
    let dir = tempfile::tempdir().expect("temporary directory");
    let file = dir.path().join("big.stf");
    let mut project = demo_project();
    let root = project.root();
    let book = project.add_workbook(root, Some("Big")).unwrap();
    let sheet = project.tree().children(book).unwrap()[0];
    // 2.3 million reals: more than 16 MB once written.
    project
        .add_column(
            sheet,
            None,
            Some("N"),
            Column::from_f64(&vec![1.5; 2_300_000]),
        )
        .unwrap();
    let (_session, mut state) = start_with_max_file_mb(project, &file, dir.path(), 16);
    state.dispatch(Action::Save);
    assert!(file.exists(), "the save succeeded");
    assert!(state.alert().is_none());
    let status = state.status().expect("a message");
    assert_eq!(status.level, StatusLevel::Warning);
    assert!(
        status.text.contains("Preferences") && status.text.contains("16 MB"),
        "unexpected warning: {}",
        status.text
    );

    // Under the cap: the usual message, without a warning.
    let (_session, mut small) = start_with_max_file_mb(
        demo_project(),
        &dir.path().join("small.stf"),
        dir.path(),
        16,
    );
    small.dispatch(Action::Save);
    let status = small.status().expect("a message");
    assert_ne!(status.level, StatusLevel::Warning);
    assert!(status.text.starts_with("Saved to"), "{}", status.text);
}

/// An unreadable calculation in the file is discarded (the project loads) but
/// reported by a status bar warning, no longer silently (#278).
#[test]
fn loading_a_file_with_an_unreadable_calculation_warns() {
    use stokesforge_analysis::correct::CorrectOp;
    use stokesforge_analysis::ops::Operation;
    use stokesforge_core::{
        CalculationDraft, Column, ColumnMeta, OutputDraft, SeriesRef, SeriesResult,
    };
    stokesforge_app_state::i18n::set_language(stokesforge_app_state::i18n::Language::English);
    let dir = tempfile::tempdir().expect("temporary directory");
    let path = dir.path().join("calculation.stf");

    let mut project = Project::new("P").unwrap();
    let book = project.add_workbook(project.root(), Some("Book")).unwrap();
    let sheet = project.tree().children(book).unwrap()[0];
    let y = project
        .add_column(sheet, None, Some("Y"), Column::from_f64(&[1.0, 2.0]))
        .unwrap();
    project
        .add_calculation(CalculationDraft {
            operation: Operation::Correct {
                op: CorrectOp::Subtract,
            },
            series: vec![SeriesRef {
                x: None,
                y,
                sigma: None,
            }],
            aux: None,
            outputs: vec![OutputDraft {
                series: 0,
                key: "corrected".into(),
                index: None,
                meta: ColumnMeta::default(),
                data: vec![0.0, 1.0],
            }],
            results: vec![SeriesResult::default()],
            keep_results: true,
            auto_recalc: true,
            low_r2_dialog: true,
        })
        .unwrap();
    stokesforge_persistence::save(&project, &path).expect("write");
    // Same length: the operation becomes unreadable without shifting a byte.
    let mut bytes = std::fs::read(&path).unwrap();
    let at = bytes
        .windows(8)
        .position(|w| w == b"subtract")
        .expect("the operation is in the file");
    bytes[at..at + 8].copy_from_slice(b"subtraXX");
    std::fs::write(&path, &bytes).unwrap();

    let (_session, mut state) = start_demo(dir.path());
    state.set_path(path.display().to_string());
    state.dispatch(Action::Load);

    assert!(state.alert().is_none(), "the project loads");
    let status = state.status().expect("a message");
    assert_eq!(status.level, StatusLevel::Warning);
    assert!(
        status.text.contains("was discarded") && status.text.contains('#'),
        "{}",
        status.text
    );
}

/// A `.stf` read error is shown through the sentence of the application
/// catalog (#47), not through the raw text of the crate below: "is damaged"
/// here, "corrupt file" in the `Display` that the CLI reads.
#[test]
fn a_stf_read_error_is_shown_through_the_catalogue() {
    // Compared in English: the test must not depend on `STOKESFORGE_LANG` or on the
    // saved preference of whoever runs it.
    stokesforge_app_state::i18n::set_language(stokesforge_app_state::i18n::Language::English);
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());

    let truncated = dir.path().join("truncated.stf");
    std::fs::write(&truncated, b"STKF\x01").expect("writing the test file");
    state.set_path(truncated.display().to_string());
    state.dispatch(Action::Load);
    let alert = state.alert().expect("a serious failure (S-M8)");
    assert_eq!(
        alert.message,
        format!(
            "{} is damaged: header truncated before the version number",
            truncated.display()
        )
    );

    let foreign = dir.path().join("foreign.stf");
    std::fs::write(&foreign, b"hello").expect("writing the test file");
    state.set_path(foreign.display().to_string());
    state.dispatch(Action::DismissAlert);
    state.dispatch(Action::Load);
    let alert = state.alert().expect("a serious failure (S-M8)");
    assert_eq!(
        alert.message,
        format!("{} is not a StokesForge file", foreign.display())
    );
}

#[test]
fn a_failing_save_reports_an_error_and_writes_nothing() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let target = dir.path().join("absent").join("project.stf");

    state.set_path(target.display().to_string());
    state.dispatch(Action::Save);

    assert_eq!(
        state.alert().map(|alert| alert.kind),
        Some(AlertKind::SaveFailed),
        "S-M8: blocking dialog"
    );
    assert!(!target.exists(), "nothing was written");
    assert!(!target.parent().is_some_and(Path::exists));
}

/// #315: a file that exists at the starting path does not make the project a
/// "saved" project — only a real load ([`AppState::mark_loaded`])
/// sets the marker. Otherwise Ctrl+S would overwrite that file unchecked.
#[test]
fn an_existing_file_does_not_make_the_project_saved_without_a_load() {
    let dir = tempfile::tempdir().unwrap();
    let existing = dir.path().join("existing.stf");
    let (writer, _) = start_demo(dir.path());
    writer.handle().save(&existing).unwrap();
    assert!(existing.is_file());

    let (_session, mut state) = start(demo_project(), &existing);
    let title = state.window_title();
    assert!(title.contains("existing.stf •"), "{title}");
    state.dispatch(Action::Quit {
        pending_edits: false,
    });
    assert_eq!(state.unsaved_pending(), Some(&UnsavedGesture::Quit));

    // Witness: the explicit load sets the marker.
    state.dispatch(Action::ConfirmUnsaved(UnsavedChoice::Cancel));
    state.mark_loaded(&stokesforge_session::LoadReport::default());
    let title = state.window_title();
    assert!(!title.contains('•'), "{title}");
    state.dispatch(Action::Quit {
        pending_edits: false,
    });
    assert!(state.unsaved_pending().is_none());
}

/// #315: a failed "Save As" restores the old path; the next Ctrl+S
/// does not write to the file that just failed.
#[test]
fn a_failed_save_as_restores_the_old_path() {
    let dir = tempfile::tempdir().unwrap();
    let (_session, mut state) = start_demo(dir.path());
    let old = dir.path().join("projet.stf");
    let target = dir.path().join("absent").join("x.stf");

    state.dispatch(Action::SaveAs {
        path: target.display().to_string(),
    });
    assert_eq!(
        state.alert().map(|alert| alert.kind),
        Some(AlertKind::SaveFailed)
    );
    assert_eq!(state.path(), old.display().to_string());

    state.dispatch(Action::DismissAlert);
    state.dispatch(Action::Save);
    assert!(old.is_file(), "Save writes the old file");
    assert!(!target.exists());
    assert!(!target.parent().is_some_and(Path::exists));
}

#[test]
fn a_successful_save_as_changes_the_path_and_marks_saved() {
    let dir = tempfile::tempdir().unwrap();
    let (_session, mut state) = start_demo(dir.path());
    let target = dir.path().join("new.stf");
    state.dispatch(Action::AddGraph);
    state.dispatch(Action::SaveAs {
        path: target.display().to_string(),
    });
    assert!(target.is_file());
    assert_eq!(state.path(), target.display().to_string());
    let title = state.window_title();
    assert!(title.contains("new.stf"), "{title}");
    assert!(!title.contains('•'), "{title}");
}

/// #315: without a path, the title does not stay empty.
#[test]
fn without_a_path_the_title_carries_a_fallback_name() {
    let (_session, state) = start(demo_project(), Path::new(""));
    let title = state.window_title();
    assert!(
        title.starts_with(&stokesforge_app_state::tr!("window-title-untitled")),
        "{title}"
    );
}

// -- 10. Columns of different lengths --

#[test]
fn reading_past_the_end_of_a_short_column_yields_a_missing_value() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let note = column_id(&state, sheet1, "Note");

    assert_eq!(state.row_count(sheet1), 5);
    // `Note` = [Some("a"), None, Some("")]: row 2 is a *present* empty
    // string, rows 3 and beyond are out of bounds — `Column::get`
    // returns `Cell::Text(None)` there, the same *absent* value as a row 1
    // explicitly written as `None`. A9 (option 2): these two natures are no
    // longer displayed the same way (before this fix, the four
    // assertions below all returned `""`).
    assert_eq!(
        state.cell_text(note, 2),
        "",
        "Some(\"\") is displayed empty"
    );
    let absente = state.cell_text(note, 3);
    assert_ne!(
        absente,
        state.cell_text(note, 2),
        "out of bounds (absent value) must display differently from \
         Some(\"\") (present empty string)"
    );
    assert_eq!(
        state.cell_text(note, 4),
        absente,
        "every out-of-bounds position displays the same thing"
    );
    assert_eq!(
        state.cell_text(note, 10_000),
        absente,
        "every out-of-bounds position displays the same thing"
    );
}

// -- 11. Rows and columns --

#[test]
fn inserting_then_undoing_rows_restores_the_tree_bit_for_bit() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let before = std::sync::Arc::clone(state.tree());

    state.dispatch(Action::InsertRows {
        sheet: sheet1,
        at: 1,
        count: 2,
    });
    assert_eq!(state.row_count(sheet1), 7);

    state.dispatch(Action::Undo);
    assert_eq!(**state.tree(), *before);
    assert_eq!(state.row_count(sheet1), 5);
}

#[test]
fn deleting_then_undoing_rows_restores_the_tree_bit_for_bit() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let before = std::sync::Arc::clone(state.tree());

    state.dispatch(Action::DeleteRows {
        sheet: sheet1,
        at: 1,
        count: 2,
    });
    assert_eq!(state.row_count(sheet1), 3);

    state.dispatch(Action::Undo);
    assert_eq!(**state.tree(), *before);
    assert_eq!(state.row_count(sheet1), 5);
}

#[test]
fn adding_then_undoing_a_column_restores_the_tree_bit_for_bit() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let before = std::sync::Arc::clone(state.tree());
    let names_before: Vec<String> = state
        .visible_columns(sheet1)
        .into_iter()
        .map(|(_, name, _)| name)
        .collect();

    state.dispatch(Action::AddColumn { sheet: sheet1 });

    let columns = state.visible_columns(sheet1);
    assert_eq!(columns.len(), names_before.len() + 1);
    let (_, new_name, new_type) = columns.last().cloned().expect("the added column");
    assert!(!new_name.is_empty(), "a name was generated");
    assert!(!names_before.contains(&new_name), "free name");
    assert_eq!(new_type, ColumnType::Numeric);

    state.dispatch(Action::Undo);
    assert_eq!(**state.tree(), *before);
    assert_eq!(state.visible_columns(sheet1).len(), names_before.len());
}

#[test]
fn adding_then_undoing_a_worksheet_restores_the_tree_bit_for_bit() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let book1 = book_named(state.tree(), "Book1");
    let before = std::sync::Arc::clone(state.tree());
    let sheets_before = state.worksheets_of(book1);

    state.dispatch(Action::AddWorksheet { book: book1 });

    let sheets_after = state.worksheets_of(book1);
    assert_eq!(sheets_after.len(), sheets_before.len() + 1);
    let (new_sheet, new_name) = sheets_after.last().cloned().expect("the added sheet");
    assert!(!new_name.is_empty(), "a name was generated");
    assert!(
        sheets_before.iter().all(|(id, _)| *id != new_sheet),
        "new sheet"
    );

    state.dispatch(Action::Undo);
    assert_eq!(**state.tree(), *before);
    assert_eq!(state.worksheets_of(book1).len(), sheets_before.len());
}

#[test]
fn creating_a_workbook_is_undoable_and_redoable() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let before = std::sync::Arc::clone(state.tree());
    let books_before = state.books();

    state.dispatch(Action::AddWorkbook);

    let after = std::sync::Arc::clone(state.tree());
    let books_after = state.books();
    assert_eq!(books_after.len(), books_before.len() + 1);
    let (book, name) = books_after.last().expect("workbook created");
    assert!(!name.is_empty(), "a name is generated");
    assert!(
        state.selected_sheet_in(*book).is_some(),
        "a new workbook contains its initial sheet"
    );
    assert_eq!(
        state.undo_label(),
        Some(undo_entry(&HistoryLabel::Macro(MacroKind::AddWorkbook)))
    );

    state.dispatch(Action::Undo);
    assert_eq!(**state.tree(), *before);

    state.dispatch(Action::Redo);
    assert_eq!(**state.tree(), *after);
}

/// The direct children of `parent`, with their name.
fn child_names(
    state: &AppState,
    parent: stokesforge_core::NodeId,
) -> Vec<(stokesforge_core::NodeId, String)> {
    state
        .tree()
        .node(parent)
        .expect("parent")
        .children()
        .iter()
        .map(|id| {
            (
                *id,
                state.tree().node(*id).expect("child").name().to_string(),
            )
        })
        .collect()
}

/// #326: Add Folder creates a named folder, undoable and redoable.
#[test]
fn creating_a_folder_is_undoable_and_redoable() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let root = state.tree().root();
    let before = std::sync::Arc::clone(state.tree());

    state.dispatch(Action::AddFolder { parent: root });

    let after = std::sync::Arc::clone(state.tree());
    assert_eq!(after.len(), before.len() + 1);
    let (folder, name) = child_names(&state, root).pop().expect("folder created");
    assert_eq!(name, "Folder1");
    assert!(matches!(
        state.tree().node(folder).unwrap().kind(),
        stokesforge_core::NodeKind::Folder { .. }
    ));
    assert_eq!(
        state.undo_label(),
        Some(undo_entry(&HistoryLabel::AddFolder { name }))
    );

    state.dispatch(Action::Undo);
    assert_eq!(**state.tree(), *before);

    state.dispatch(Action::Redo);
    assert_eq!(**state.tree(), *after);
}

/// #326: two creations in a row do not collide (sibling uniqueness).
#[test]
fn two_folders_get_distinct_names() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let root = state.tree().root();

    state.dispatch(Action::AddFolder { parent: root });
    state.dispatch(Action::AddFolder { parent: root });

    assert!(state.status().is_none_or(|status| !status.is_error()));
    let names: Vec<_> = child_names(&state, root)
        .into_iter()
        .map(|(_, name)| name)
        .filter(|name| name.starts_with("Folder"))
        .collect();
    assert_eq!(names, ["Folder1", "Folder2"]);
}

/// #326: the name is searched among the children of the target folder, not of the root.
#[test]
fn a_folder_is_created_inside_the_chosen_parent() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let root = state.tree().root();
    state.dispatch(Action::AddFolder { parent: root });
    let (outer, _) = child_names(&state, root).pop().expect("Folder1");
    let root_children = child_names(&state, root).len();

    state.dispatch(Action::AddFolder { parent: outer });

    assert!(state.status().is_none_or(|status| !status.is_error()));
    let inner = child_names(&state, outer);
    assert_eq!(
        inner.iter().map(|(_, n)| n.as_str()).collect::<Vec<_>>(),
        ["Folder1"],
        "free in the subfolder, even if the root already carries Folder1"
    );
    assert_eq!(child_names(&state, root).len(), root_children);
}

/// #326: a vanished parent fails in the status bar, without a history entry.
#[test]
fn a_folder_in_a_removed_parent_fails_without_history() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let root = state.tree().root();
    state.dispatch(Action::AddFolder { parent: root });
    let (gone, _) = child_names(&state, root).pop().expect("folder");
    state.dispatch(Action::DeleteObjects(vec![gone]));
    assert!(state.tree().node(gone).is_none(), "the folder is removed");
    let label = state.undo_label();
    let revision = state.snapshot().revision;

    state.dispatch(Action::AddFolder { parent: gone });

    assert!(state.status().is_some_and(|status| status.is_error()));
    assert_eq!(state.undo_label(), label);
    assert_eq!(state.snapshot().revision, revision);
}

#[test]
fn row_actions_on_something_that_is_not_a_sheet_are_refused() {
    let dir = tempfile::tempdir().expect("temporary directory");
    // A node that is not a sheet: a graph, for example.
    let mut project = Project::new("Empty").expect("empty project");
    let root = project.root();
    let graph = project
        .add_graph(root, Some("Graph1"), Default::default())
        .expect("a graph");
    let (_session, mut state) = start(project, &dir.path().join("empty.stf"));

    assert!(state.visible_columns(graph).is_empty());
    assert_eq!(state.row_count(graph), 0);

    let revision = state.snapshot().revision;
    state.dispatch(Action::InsertRows {
        sheet: graph,
        at: 0,
        count: 1,
    });
    assert!(state.status().is_some_and(|status| status.is_error()));
    assert_eq!(state.snapshot().revision, revision);

    state.dispatch(Action::AddColumn { sheet: graph });
    assert!(state.status().is_some_and(|status| status.is_error()));
    assert_eq!(state.snapshot().revision, revision);
}

#[test]
fn selecting_something_that_is_not_a_sheet_is_refused() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let book1 = book_named(state.tree(), "Book1");
    let selected = state.selected_sheet_in(book1);
    let root = state.tree().root();

    state.dispatch(Action::SelectSheet(root));
    assert!(state.status().is_some_and(|status| status.is_error()));
    assert_eq!(
        state.selected_sheet_in(book1),
        selected,
        "selection unchanged"
    );

    state.dispatch(Action::SelectSheet(NodeId::from_raw(9_999)));
    assert!(state.status().is_some_and(|status| status.is_error()));
    assert_eq!(state.selected_sheet_in(book1), selected);
}

/// The text of the Undo menu for `label`, in the current language: the test
/// compares the **structure** (which entry, which parameters), not a language;
/// the English sentences are held by `tests/history_text.rs`.
fn undo_entry(label: &HistoryLabel) -> String {
    stokesforge_app_state::tr!("history-undo-entry", label = history_text(label))
}

/// The text of the Redo menu for `label`, like [`undo_entry`].
fn redo_entry(label: &HistoryLabel) -> String {
    stokesforge_app_state::tr!("history-redo-entry", label = history_text(label))
}

// -- 12. Labels --

#[test]
fn labels_prefix_the_history_entry_and_vanish_when_the_action_is_impossible() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");

    assert_eq!(state.undo_label(), None);
    assert_eq!(state.redo_label(), None);

    state.dispatch(Action::AddColumn { sheet: sheet1 });
    let undo = state.undo_label().expect("an undo label");
    assert_eq!(undo, undo_entry(&HistoryLabel::AddColumn));
    assert_eq!(state.redo_label(), None, "nothing to redo");

    state.dispatch(Action::Undo);
    let redo = state.redo_label().expect("a redo label");
    assert_eq!(redo, redo_entry(&HistoryLabel::AddColumn));
    assert_eq!(state.undo_label(), None);
}

// -- Miscellaneous: the side explorer --

#[test]
fn sheets_lists_every_worksheet_with_a_readable_path() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, state) = start_demo(dir.path());

    let sheets = state.sheets();
    let paths: Vec<&str> = sheets.iter().map(|(_, path)| path.as_str()).collect();
    assert_eq!(paths, vec!["Book1 / Sheet1", "Book1 / Sheet2"]);
    assert_eq!(sheets[0].0, sheet_named(state.tree(), "Sheet1"));
    assert_eq!(sheets[1].0, sheet_named(state.tree(), "Sheet2"));
}

#[test]
fn selecting_a_sheet_updates_its_books_selection() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let book1 = book_named(state.tree(), "Book1");
    let sheet2 = sheet_named(state.tree(), "Sheet2");

    state.dispatch(Action::SelectSheet(sheet2));

    assert_eq!(state.selected_sheet_in(book1), Some(sheet2));
    let names: Vec<String> = state
        .visible_columns(sheet2)
        .into_iter()
        .map(|(_, name, _)| name)
        .collect();
    assert_eq!(names, vec!["Short".to_string()]);
    assert_eq!(state.row_count(sheet2), 2);
}

/// Two workbooks open at the same time (two `Tab::Book`) do not share
/// their selected sheet: selecting a sheet in one must
/// never affect the other — the reason for [`AppState::
/// selected_sheet_in`] being scoped by workbook rather than a global selection.
#[test]
fn selecting_a_sheet_in_one_book_does_not_affect_another_book() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let mut project = demo_project();
    let root = project.root();
    let book2 = project
        .add_workbook(root, Some("Book2"))
        .expect("workbook Book2");
    let book2_sheet2 = project
        .add_worksheet(book2, Some("Other"))
        .expect("worksheet Other");

    let (_session, mut state) = start(project, &dir.path().join("project.stf"));
    let book1 = book_named(state.tree(), "Book1");
    let book1_sheet2 = sheet_named(state.tree(), "Sheet2");

    state.dispatch(Action::SelectSheet(book1_sheet2));
    state.dispatch(Action::SelectSheet(book2_sheet2));

    assert_eq!(state.selected_sheet_in(book1), Some(book1_sheet2));
    assert_eq!(state.selected_sheet_in(book2), Some(book2_sheet2));
}

// -- Miscellaneous: the cell input buffer --

#[test]
fn the_cell_buffer_only_reaches_the_model_on_commit() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let time_col = column_id(&state, sheet1, "Time");
    let revision = state.snapshot().revision;

    state.begin_edit(time_col, 0);
    assert_eq!(
        state.editing().map(|edit| edit.buffer.as_str()),
        Some("0"),
        "the buffer starts from the displayed text"
    );
    if let Some(edit) = state.editing_mut() {
        edit.buffer = "7.25".to_string();
    }
    assert_eq!(
        state.snapshot().revision,
        revision,
        "typing does not touch the model: otherwise one history entry per character"
    );

    state.commit_edit();
    assert_eq!(cell(&state, time_col, 0), Cell::Float(7.25));
    assert!(state.editing().is_none(), "the edit is closed");
    assert!(state.snapshot().revision > revision);
}

#[test]
fn cancelling_an_edit_discards_the_buffer() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let time_col = column_id(&state, sheet1, "Time");
    let revision = state.snapshot().revision;

    state.begin_edit(time_col, 0);
    if let Some(edit) = state.editing_mut() {
        edit.buffer = "7.25".to_string();
    }
    state.cancel_edit();

    assert!(state.editing().is_none());
    assert_eq!(cell(&state, time_col, 0), Cell::Float(0.0));
    assert_eq!(state.snapshot().revision, revision);
}

// -- Miscellaneous: the grid selection (mouse + Ctrl+C/X/V) --
//
// `stokesforge_app::app` (egui) translates click/drag into `AppState::select_cell`/
// `extend_selection`, then Ctrl+C/Ctrl+Shift+C/X/V/Ctrl+Shift+V into
// `AppState::copy_selection`/`AppState::copy_full_columns`/
// `Action::ClearSelectedCells`/`Action::Paste`/`Action::PasteAbsolute` —
// everything after this translation point is testable here, without a window.
// Bug "copy/cut/paste does not work with a single-cell
// selection": a selection set by a plain click
// ([`AppState::select_cell`]) is a 1×1 rectangle like any other for
// `Selection` (see `crate::selection::Selection::single_cell` and its unit
// tests) — the three tests that follow check it at the level where
// `stokesforge_app::app` stops.

#[test]
fn a_single_cell_selection_copies_exactly_that_cell() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let time_col = column_id(&state, sheet1, "Time");

    state.select_cell(sheet1, time_col, 2);
    assert!(state.is_selected(sheet1, time_col, 2));

    let signal = column_id(&state, sheet1, "Signal");
    assert!(
        !state.is_selected(sheet1, signal, 2),
        "a single cell is selected, not the whole row"
    );

    assert_eq!(
        state.copy_selection(sheet1),
        Some("1".to_string()),
        "Ctrl+C on a single-cell selection copies exactly its text, nothing else"
    );
}

#[test]
fn clearing_a_single_cell_selection_empties_only_that_cell() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let time_col = column_id(&state, sheet1, "Time");

    state.select_cell(sheet1, time_col, 2);
    state.dispatch(Action::ClearSelectedCells { sheet: sheet1 });

    assert_eq!(cell(&state, time_col, 2), Cell::Float(f64::NAN));
    assert_eq!(
        cell(&state, time_col, 0),
        Cell::Float(0.0),
        "Ctrl+X/Backspace on a single cell does not touch the others"
    );
    assert_eq!(cell(&state, time_col, 1), Cell::Float(0.5));
}

#[test]
fn pasting_onto_a_single_cell_selection_writes_from_its_anchor() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let time_col = column_id(&state, sheet1, "Time");
    let signal = column_id(&state, sheet1, "Signal");

    // Single-cell selection on `Time`, row 1 — the paste must
    // stretch from this anchor, over as many columns as the pasted TSV
    // carries (here two: `Time` and `Signal`), not only the selected
    // cell.
    state.select_cell(sheet1, time_col, 1);
    state.dispatch(Action::Paste {
        sheet: sheet1,
        text: "9\t99\n".to_string(),
    });

    assert_eq!(cell(&state, time_col, 1), Cell::Float(9.0));
    assert_eq!(cell(&state, signal, 1), Cell::Float(99.0));
    // The rest of the column did not move.
    assert_eq!(cell(&state, time_col, 0), Cell::Float(0.0));
}

/// Regression: pasting a non-numeric text into a `Numeric` column
/// silently wrote `NaN` (the cell looked cleared) without ever
/// widening the column to `Mixed` — unlike the same input typed on
/// the keyboard (`non_numeric_input_promotes_the_column_to_mixed`). The paste
/// must behave exactly the same: it is the same tolerance,
/// `Command::SetCellsPromoting` rather than a `NaN` that swallows the pasted text.
#[test]
fn pasting_non_numeric_text_onto_a_numeric_cell_promotes_the_column_to_mixed() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let time_col = column_id(&state, sheet1, "Time");
    let voisin = cell(&state, time_col, 0);

    state.select_cell(sheet1, time_col, 1);
    state.dispatch(Action::Paste {
        sheet: sheet1,
        text: "n/a".to_string(),
    });

    assert_eq!(
        column_type(&state, time_col),
        ColumnType::Mixed,
        "the paste must promote the column as a keyboard input would"
    );
    assert_eq!(state.cell_text(time_col, 1), "n/a");
    assert_eq!(
        cell(&state, time_col, 0),
        voisin,
        "the other rows keep their number"
    );

    // A single history entry for the promotion **and** the write.
    state.dispatch(Action::Undo);
    assert_eq!(column_type(&state, time_col), ColumnType::Numeric);
    assert_eq!(cell(&state, time_col, 1), Cell::Float(0.5));
}

/// Same regression, for a rectangle pasted over several rows: a single
/// non-numeric value among the pasted rows is enough to promote
/// the whole write to `Mixed`, the numeric values of the same paste
/// remaining plottable numbers.
#[test]
fn pasting_a_mixed_numeric_and_text_block_promotes_the_column_once() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let time_col = column_id(&state, sheet1, "Time");

    state.select_cell(sheet1, time_col, 0);
    state.dispatch(Action::Paste {
        sheet: sheet1,
        text: "7\nn/a\n9\n".to_string(),
    });

    assert_eq!(column_type(&state, time_col), ColumnType::Mixed);
    assert_eq!(cell(&state, time_col, 0), Cell::Float(7.0));
    assert_eq!(state.cell_text(time_col, 1), "n/a");
    assert_eq!(cell(&state, time_col, 2), Cell::Float(9.0));
}

// -- Input and paste: a single reading rule (#258) --

/// #258, reproduction: a pasted date keeps its date value, like a typed
/// date. Two lines `2026-10-15` then `n/a` pasted into a DateTime
/// column widen it to Mixed; the first cell stays a timestamp, the
/// second is text, and a single Undo gives the DateTime column back.
#[test]
fn a_pasted_date_keeps_its_value_when_the_column_widens_to_mixed() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let time_col = column_id(&state, sheet1, "Time");
    state.dispatch(Action::SetColumnType {
        column: time_col,
        column_type: ColumnType::DateTime,
    });
    let expected = stokesforge_core::datetime::parse_datetime("2026-10-15").expect("date");

    state.select_cell(sheet1, time_col, 0);
    state.dispatch(Action::Paste {
        sheet: sheet1,
        text: "2026-10-15\nn/a".to_string(),
    });

    assert_eq!(column_type(&state, time_col), ColumnType::Mixed);
    assert_eq!(cell(&state, time_col, 0), Cell::Float(expected));
    assert_eq!(
        cell(&state, time_col, 1),
        Cell::Text(Some("n/a".to_string()))
    );

    // Witness: the same sequence typed cell by cell gives the same cells.
    let (_other, mut typed) = start_demo(dir.path());
    let typed_time = column_id(&typed, sheet_named(typed.tree(), "Sheet1"), "Time");
    typed.dispatch(Action::SetColumnType {
        column: typed_time,
        column_type: ColumnType::DateTime,
    });
    for (row, text) in ["2026-10-15", "n/a"].into_iter().enumerate() {
        typed.dispatch(Action::CommitCell {
            column: typed_time,
            row,
            text: text.to_string(),
        });
    }
    assert_eq!(cell(&typed, typed_time, 0), cell(&state, time_col, 0));
    assert_eq!(cell(&typed, typed_time, 1), cell(&state, time_col, 1));

    state.dispatch(Action::Undo);
    assert_eq!(column_type(&state, time_col), ColumnType::DateTime);
    assert_eq!(cell(&state, time_col, 0), Cell::Float(0.0));
}

/// #258: typed `inf` and `nan` stay text, even in an already
/// Mixed column (before, `MixedCell::from_text` read them as numbers); the column stays
/// Mixed and an infinity never enters the data through the grid.
#[test]
fn typing_or_pasting_inf_and_nan_in_a_mixed_column_keeps_text() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let note = column_id(&state, sheet1, "Note");
    assert_eq!(column_type(&state, note), ColumnType::Mixed);

    state.dispatch(Action::CommitCell {
        column: note,
        row: 0,
        text: "inf".to_string(),
    });
    assert_eq!(state.cell_text(note, 0), "inf");
    assert_eq!(cell(&state, note, 0), Cell::Text(Some("inf".to_string())));
    assert_eq!(column_type(&state, note), ColumnType::Mixed);

    state.select_cell(sheet1, note, 1);
    state.dispatch(Action::Paste {
        sheet: sheet1,
        text: "-Infinity\nnan".to_string(),
    });
    assert_eq!(
        cell(&state, note, 1),
        Cell::Text(Some("-Infinity".to_string()))
    );
    assert_eq!(cell(&state, note, 2), Cell::Text(Some("nan".to_string())));
    assert_eq!(column_type(&state, note), ColumnType::Mixed);
}

/// #258: after a paste, a Mixed column whose present cells
/// are all numbers goes back to Numeric, as after a typed input. The demo's `Note`
/// is `[Some("a"), None, Some("")]`: the paste overwrites the three
/// cells (the `Some("")` would block the repromotion). A single Undo reverts the
/// paste **and** the repromotion.
#[test]
fn pasting_numbers_over_a_mixed_column_makes_it_numeric_again_in_one_undo() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let note = column_id(&state, sheet1, "Note");

    state.select_cell(sheet1, note, 0);
    state.dispatch(Action::Paste {
        sheet: sheet1,
        text: "1\n\n2".to_string(),
    });

    assert_eq!(column_type(&state, note), ColumnType::Numeric);
    assert_eq!(cell(&state, note, 0), Cell::Float(1.0));
    assert!(cell(&state, note, 1).is_missing());
    assert_eq!(cell(&state, note, 2), Cell::Float(2.0));
    assert_eq!(
        state.undo_label(),
        Some(undo_entry(&HistoryLabel::Macro(MacroKind::PasteCells))),
        "paste and repromotion form one entry"
    );

    state.dispatch(Action::Undo);
    assert_eq!(column_type(&state, note), ColumnType::Mixed);
    assert_eq!(cell(&state, note, 0), Cell::Text(Some("a".to_string())));
    assert_eq!(cell(&state, note, 2), Cell::Text(Some(String::new())));
}

/// #258: a paste of several columns that repromotes `Note` stays **one**
/// history entry (the repromotion happens before the group closes).
#[test]
fn a_multi_column_paste_with_a_repromotion_is_one_undo() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let signal = column_id(&state, sheet1, "Signal");
    let note = column_id(&state, sheet1, "Note");
    while state.can_undo() {
        state.dispatch(Action::Undo);
    }

    state.select_cell(sheet1, signal, 0);
    state.dispatch(Action::Paste {
        sheet: sheet1,
        text: "5\t1\n6\t\n7\t2".to_string(),
    });
    assert_eq!(column_type(&state, note), ColumnType::Numeric);
    assert_eq!(cell(&state, signal, 0), Cell::Float(5.0));
    assert_eq!(cell(&state, note, 2), Cell::Float(2.0));

    state.dispatch(Action::Undo);
    assert_eq!(column_type(&state, note), ColumnType::Mixed);
    assert_eq!(cell(&state, note, 0), Cell::Text(Some("a".to_string())));
    assert_eq!(cell(&state, signal, 0), Cell::Float(10.0));
    assert!(!state.can_undo(), "a single Undo reverted everything");
}

/// #258, the two other paste points: a copied cell pasted
/// (`GridClipboard::Text`) and a whole column pasted
/// (`GridClipboard::FullColumns`) also repromote, in one entry.
#[test]
fn single_value_and_full_column_pastes_also_repromote_a_mixed_column() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let time_col = column_id(&state, sheet1, "Time");
    let signal = column_id(&state, sheet1, "Signal");

    // One cell: `Signal` becomes Mixed, receives an "n/a" then a number.
    state.dispatch(Action::SetColumnType {
        column: signal,
        column_type: ColumnType::Mixed,
    });
    state.select_cell(sheet1, signal, 1);
    state.dispatch(Action::Paste {
        sheet: sheet1,
        text: "n/a".to_string(),
    });
    assert_eq!(column_type(&state, signal), ColumnType::Mixed);
    state.select_cell(sheet1, time_col, 2);
    let copied = state.copy_selection(sheet1).expect("non-empty selection");
    state.select_cell(sheet1, signal, 1);
    state.dispatch(Action::Paste {
        sheet: sheet1,
        text: copied,
    });
    assert_eq!(column_type(&state, signal), ColumnType::Numeric);
    assert_eq!(cell(&state, signal, 1), Cell::Float(1.0));
    state.dispatch(Action::Undo);
    assert_eq!(column_type(&state, signal), ColumnType::Mixed);
    assert_eq!(cell(&state, signal, 1), Cell::Text(Some("n/a".to_string())));

    // A whole column: `Time` pasted onto the mixed `Note`.
    let note = column_id(&state, sheet1, "Note");
    state.select_cell(sheet1, time_col, 0);
    let full_columns = state
        .copy_full_columns(sheet1)
        .expect("non-empty selection");
    state.select_cell(sheet1, note, 0);
    state.dispatch(Action::Paste {
        sheet: sheet1,
        text: full_columns,
    });
    assert_eq!(column_type(&state, note), ColumnType::Numeric);
    assert_eq!(cell(&state, note, 4), Cell::Float(2.0));
    state.dispatch(Action::Undo);
    assert_eq!(column_type(&state, note), ColumnType::Mixed);
    assert_eq!(cell(&state, note, 0), Cell::Text(Some("a".to_string())));
}

/// #258: a paste onto a Mixed column that stays Mixed does not open a
/// group for nothing — it is the same simple entry as before.
#[test]
fn a_paste_that_leaves_a_mixed_column_mixed_is_a_plain_entry() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let note = column_id(&state, sheet1, "Note");

    state.select_cell(sheet1, note, 1);
    state.dispatch(Action::Paste {
        sheet: sheet1,
        text: "x\ny".to_string(),
    });
    assert_eq!(column_type(&state, note), ColumnType::Mixed);
    assert_eq!(
        state.undo_label(),
        Some(undo_entry(&HistoryLabel::SetCells {
            column: Some(ColumnRef(2))
        }))
    );
}

/// Validates a header field through the same path as user input
/// (`begin_header_edit`/`commit_header_edit`), to prepare a field
/// selection in the mixed copy/paste tests below.
fn set_header_field(state: &mut AppState, column: NodeId, field: HeaderField, text: &str) {
    state.begin_header_edit(column, field);
    if let Some(edit) = state.header_editing_mut() {
        edit.buffer = text.to_string();
    }
    state.commit_header_edit();
}

/// Historical bug (three successive fixes): a selection copied across
/// the header and the data ("mixed rectangle",
/// `paste_tsv` back then) dumped the header field rows into data
/// rows, shifting everything after. The new model makes this
/// scenario **structurally impossible** rather than fixing it
/// case by case: a drag starting from a header field that reaches a
/// data cell stops at the Formula field (see `crate::selection`),
/// so it can never again produce a mixed copy. This test checks it at the
/// level where `stokesforge_app::app` stops: the clamped selection copies only
/// fields, and pasting that result onto a data cell (different
/// kinds) pastes nothing rather than spreading badly.
#[test]
fn extending_a_header_selection_into_data_clamps_instead_of_mixing_a_copy() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let time_col = column_id(&state, sheet1, "Time");
    let signal = column_id(&state, sheet1, "Signal");

    set_header_field(&mut state, time_col, HeaderField::Unit, "nm");
    set_header_field(&mut state, time_col, HeaderField::Comment, "a");
    // Formula deliberately left empty, to check that an empty field is
    // copied properly (and does not break the format).

    // Drag starting from the Unit field toward a data cell: the
    // selection stops at Formula, it never touches row 2.
    state.select_header_field(sheet1, time_col, HeaderField::Unit);
    state.extend_selection(sheet1, time_col, 2);
    assert!(state.is_header_field_selected(sheet1, time_col, HeaderField::Unit));
    assert!(state.is_header_field_selected(sheet1, time_col, HeaderField::Comment));
    assert!(state.is_header_field_selected(sheet1, time_col, HeaderField::Formula));
    assert!(
        !state.is_selected(sheet1, time_col, 2),
        "the drag no longer crosses the header/data boundary"
    );

    let text = state.copy_selection(sheet1).expect("non-empty selection");
    assert_eq!(
        text, "nm\na\n\n",
        "3 plain field rows, nothing more: the copy no longer carries \
         a data row"
    );

    let signal_data_before = [
        cell(&state, signal, 0),
        cell(&state, signal, 1),
        cell(&state, signal, 2),
    ];

    // Pasted onto a data cell of `Signal`: different kinds
    // (copied fields, anchor on a data cell), nothing is pasted (rule 2 of
    // the behavior matrix, see `stokesforge_app_state::state`).
    state.select_cell(sheet1, signal, 1);
    state.dispatch(Action::Paste {
        sheet: sheet1,
        text,
    });

    let meta = state.column_meta(signal);
    assert_eq!(meta.unit, "");
    assert_eq!(meta.comment, "");
    assert_eq!(cell(&state, signal, 0), signal_data_before[0]);
    assert_eq!(cell(&state, signal, 1), signal_data_before[1]);
    assert_eq!(cell(&state, signal, 2), signal_data_before[2]);
}

/// A pure data copy/paste (no header field in the
/// selection) pastes exactly like a reference spreadsheet: shifted onto the
/// anchor, nothing else touched.
#[test]
fn pure_data_copy_paste_keeps_working_without_header_markers() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let time_col = column_id(&state, sheet1, "Time");
    let signal = column_id(&state, sheet1, "Signal");

    state.select_cell(sheet1, time_col, 0);
    state.extend_selection(sheet1, signal, 1);
    let text = state.copy_selection(sheet1).expect("non-empty selection");
    assert_eq!(text, "0\t10\n0.5\t\n");

    state.select_cell(sheet1, time_col, 3);
    state.dispatch(Action::Paste {
        sheet: sheet1,
        text,
    });

    assert_eq!(cell(&state, time_col, 3), Cell::Float(0.0));
    assert_eq!(cell(&state, signal, 3), Cell::Float(10.0));
    assert_eq!(cell(&state, time_col, 4), Cell::Float(0.5));
    assert!(matches!(cell(&state, signal, 4), Cell::Float(v) if v.is_nan()));
}

/// #311: an ABSENT cell (row shorter than the target column) keeps the
/// existing value; only the present cell is written.
#[test]
fn a_pasted_row_shorter_than_the_others_keeps_the_existing_cell() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let time_col = column_id(&state, sheet1, "Time");
    let signal = column_id(&state, sheet1, "Signal");

    state.select_cell(sheet1, time_col, 2);
    state.dispatch(Action::Paste {
        sheet: sheet1,
        text: "1\t2\n3\n4\t5".to_string(),
    });
    assert_eq!(cell(&state, time_col, 2), Cell::Float(1.0));
    assert_eq!(cell(&state, signal, 2), Cell::Float(2.0));
    assert_eq!(cell(&state, time_col, 3), Cell::Float(3.0));
    assert_eq!(
        cell(&state, signal, 3),
        Cell::Float(40.0),
        "absent cell between two present cells: the existing value stays"
    );
    assert_eq!(cell(&state, signal, 4), Cell::Float(5.0));
}

/// #311: a PRESENT and empty cell (`"3\t"`) stays a missing value.
#[test]
fn a_present_but_empty_pasted_cell_is_still_a_missing_value() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let time_col = column_id(&state, sheet1, "Time");
    let signal = column_id(&state, sheet1, "Signal");

    state.select_cell(sheet1, time_col, 2);
    state.dispatch(Action::Paste {
        sheet: sheet1,
        text: "1\t2\n3\t".to_string(),
    });
    assert_eq!(cell(&state, time_col, 3), Cell::Float(3.0));
    assert!(matches!(cell(&state, signal, 3), Cell::Float(v) if v.is_nan()));
}

/// #311: the last short row does not lengthen the shortest column.
#[test]
fn a_short_last_row_does_not_lengthen_the_shorter_column() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let time_col = column_id(&state, sheet1, "Time");
    let signal = column_id(&state, sheet1, "Signal");
    let len = |state: &AppState, id| {
        state
            .tree()
            .node(id)
            .and_then(|n| n.as_column())
            .map(|c| c.len())
    };

    state.select_cell(sheet1, time_col, 4);
    state.dispatch(Action::Paste {
        sheet: sheet1,
        text: "1\t2\n3".to_string(),
    });
    assert_eq!(
        len(&state, time_col),
        Some(6),
        "Time receives its two cells"
    );
    assert_eq!(len(&state, signal), Some(5), "Signal receives only its own");
    assert_eq!(cell(&state, signal, 4), Cell::Float(2.0));
}

/// #311: same paste onto a `Mixed` column — the absent cell keeps its
/// cell as is, and the whole stays a single history entry.
#[test]
fn a_short_row_pasted_over_a_mixed_column_keeps_its_cell_in_one_undo() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let time_col = column_id(&state, sheet1, "Time");
    let signal = column_id(&state, sheet1, "Signal");
    let note = column_id(&state, sheet1, "Note");
    while state.can_undo() {
        state.dispatch(Action::Undo);
    }

    state.select_cell(sheet1, time_col, 0);
    state.dispatch(Action::Paste {
        sheet: sheet1,
        text: "1\t2\tx\n3\t4\n5\t6\ty".to_string(),
    });
    assert_eq!(cell(&state, note, 0), Cell::Text(Some("x".to_string())));
    assert_eq!(
        cell(&state, note, 1),
        Cell::Text(None),
        "absent cell: the existing Mixed cell stays"
    );
    assert_eq!(cell(&state, note, 2), Cell::Text(Some("y".to_string())));
    assert_eq!(cell(&state, signal, 1), Cell::Float(4.0));

    state.dispatch(Action::Undo);
    assert_eq!(cell(&state, note, 0), Cell::Text(Some("a".to_string())));
    assert_eq!(cell(&state, note, 2), Cell::Text(Some(String::new())));
    assert_eq!(cell(&state, time_col, 0), Cell::Float(0.0));
    assert_eq!(cell(&state, signal, 0), Cell::Float(10.0));
    assert!(!state.can_undo(), "a single Undo reverted everything");
}

/// #311: more columns copied than the sheet has from the anchor:
/// those that fit are written, the status counts the others.
#[test]
fn a_paste_wider_than_the_sheet_reports_the_dropped_columns() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let time_col = column_id(&state, sheet1, "Time");
    let signal = column_id(&state, sheet1, "Signal");
    let note = column_id(&state, sheet1, "Note");

    state.select_cell(sheet1, signal, 0);
    state.dispatch(Action::Paste {
        sheet: sheet1,
        text: "1\t2\t3\n4\t5\t6".to_string(),
    });
    assert_eq!(cell(&state, signal, 0), Cell::Float(1.0));
    assert_eq!(cell(&state, note, 0), Cell::Float(2.0));
    assert_eq!(cell(&state, time_col, 0), Cell::Float(0.0));
    let status = state.status().expect("a message");
    assert!(!status.is_error());
    assert!(
        status.text.contains("One column"),
        "unexpected status: {}",
        status.text
    );
}

/// #311: a paste that fits the width sets no overflow status.
#[test]
fn a_paste_that_fits_the_sheet_width_reports_nothing() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let time_col = column_id(&state, sheet1, "Time");

    state.select_cell(sheet1, time_col, 0);
    state.dispatch(Action::Paste {
        sheet: sheet1,
        text: "1\t2\t3\n4\t5\t6".to_string(),
    });
    assert!(
        state
            .status()
            .is_none_or(|status| !status.text.contains("did not fit")),
        "no overflow status"
    );
}

// -- #311 (E2, E4): decimal comma notice, overflowing header fields --

fn status_text(state: &AppState) -> String {
    state.status().map(|s| s.text.clone()).unwrap_or_default()
}

/// #311 (E2): "1,234" typed into a Numeric cell is still read as 1.234 (R6),
/// and the status bar says so; "1,5" is unambiguous: no notice.
#[test]
fn typing_a_thousands_looking_number_in_a_numeric_cell_notices_the_decimal_comma() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let time_col = column_id(&state, sheet1, "Time");

    state.dispatch(Action::CommitCell {
        column: time_col,
        row: 1,
        text: "1,234".to_string(),
    });
    assert_eq!(cell(&state, time_col, 1), Cell::Float(1.234));
    let status = state.status().expect("a notice");
    assert!(!status.is_error());
    assert!(
        status.text.contains("1,234") && status.text.contains("1.234"),
        "unexpected notice: {}",
        status.text
    );

    state.dispatch(Action::CommitCell {
        column: time_col,
        row: 2,
        text: "1,5".to_string(),
    });
    assert_eq!(cell(&state, time_col, 2), Cell::Float(1.5));
    assert!(
        !status_text(&state).contains("decimal comma"),
        "no notice for 1,5: {}",
        status_text(&state)
    );
}

/// #311 (E2): the notice also applies to a cell of a Mixed column.
#[test]
fn the_decimal_comma_notice_also_applies_to_a_mixed_cell() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let time_col = column_id(&state, sheet1, "Time");
    state.dispatch(Action::CommitCell {
        column: time_col,
        row: 0,
        text: "n/a".to_string(),
    });
    state.dispatch(Action::CommitCell {
        column: time_col,
        row: 1,
        text: "12,345".to_string(),
    });
    assert!(
        status_text(&state).contains("decimal comma"),
        "notice expected: {}",
        status_text(&state)
    );
}

/// #311 (E2): the paste does not set the notice (cell input only).
#[test]
fn pasting_a_thousands_looking_number_does_not_notice() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let time_col = column_id(&state, sheet1, "Time");
    state.select_cell(sheet1, time_col, 0);
    state.dispatch(Action::Paste {
        sheet: sheet1,
        text: "1,234".to_string(),
    });
    assert!(!status_text(&state).contains("decimal comma"));
}

/// Letters displayed for a column's formula (#321: the formula is
/// stored by identifiers, the letters are only a display).
fn formula_letters(state: &AppState, column: NodeId) -> Option<String> {
    state.tree().formula_letters(column)
}

/// Copy of three header rows (Long Name, Unit, Comment) of a
/// column, ready to paste; returns the copied text.
fn copy_three_header_rows(state: &mut AppState, sheet: NodeId, column: NodeId) -> String {
    set_header_field(state, column, HeaderField::LongName, "A");
    set_header_field(state, column, HeaderField::Unit, "u");
    set_header_field(state, column, HeaderField::Comment, "c");
    state.select_header_field(sheet, column, HeaderField::LongName);
    state.extend_header_field_selection(sheet, column, HeaderField::Comment);
    state.copy_selection(sheet).expect("non-empty selection")
}

/// #311 (E4): three rows pasted with the first on Formula: Formula
/// is written, the other two rows are counted in the status, the whole
/// in a single history entry.
#[test]
fn a_header_field_paste_anchored_low_counts_the_rows_beyond_formula() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let time_col = column_id(&state, sheet1, "Time");
    let note = column_id(&state, sheet1, "Note");
    let text = copy_three_header_rows(&mut state, sheet1, time_col);

    state.select_header_field(sheet1, note, HeaderField::Formula);
    state.dispatch(Action::Paste {
        sheet: sheet1,
        text,
    });
    assert_eq!(formula_letters(&state, note).as_deref(), Some("A"));
    assert_eq!(state.column_meta(note).unit, "", "row dropped");
    let status = state.status().expect("a status");
    assert!(!status.is_error(), "{}", status.text);
    assert!(
        status.text.contains("2 header fields did not fit"),
        "unexpected status: {}",
        status.text
    );
    state.dispatch(Action::Undo);
    assert_eq!(formula_letters(&state, note), None, "one Undo is enough");
}

/// #311 (E4): anchored on Long Name, the three rows fit: no status.
#[test]
fn a_header_field_paste_anchored_on_long_name_reports_nothing() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let time_col = column_id(&state, sheet1, "Time");
    let note = column_id(&state, sheet1, "Note");
    let text = copy_three_header_rows(&mut state, sheet1, time_col);

    state.select_header_field(sheet1, note, HeaderField::LongName);
    state.dispatch(Action::Paste {
        sheet: sheet1,
        text,
    });
    assert_eq!(state.column_meta(note).comment, "c");
    assert!(
        !status_text(&state).contains("did not fit"),
        "unexpected status: {}",
        status_text(&state)
    );
}

/// #311 (E4): columns and fields that overflow together: a single message.
#[test]
fn columns_and_header_fields_dropped_together_give_one_combined_notice() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let time_col = column_id(&state, sheet1, "Time");
    let signal = column_id(&state, sheet1, "Signal");
    let note = column_id(&state, sheet1, "Note");
    for column in [time_col, signal] {
        set_header_field(&mut state, column, HeaderField::LongName, "A");
        set_header_field(&mut state, column, HeaderField::Unit, "u");
    }
    state.select_header_field(sheet1, time_col, HeaderField::LongName);
    state.extend_header_field_selection(sheet1, signal, HeaderField::Unit);
    let text = state.copy_selection(sheet1).expect("non-empty selection");

    state.select_header_field(sheet1, note, HeaderField::Formula);
    state.dispatch(Action::Paste {
        sheet: sheet1,
        text,
    });
    let status = state.status().expect("a status");
    assert!(!status.is_error(), "{}", status.text);
    assert!(
        status
            .text
            .contains("1 column(s) and 1 header field(s) did not fit"),
        "unexpected status: {}",
        status.text
    );
}

/// #259: a text cell that carries a tab or a line break is
/// pasted in one piece into another session (hence through the clipboard
/// text alone, as from a spreadsheet) — nothing slides onto the
/// following cells. The copied text is in Excel/LibreOffice format.
#[test]
fn a_multiline_text_cell_pastes_into_another_session_without_shifting_anything() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut source) = start_demo(dir.path());
    let sheet1 = sheet_named(source.tree(), "Sheet1");
    let signal = column_id(&source, sheet1, "Signal");
    let note = column_id(&source, sheet1, "Note");
    let multiline = "line 1\nline 2\r\nTab\there \"quoted\"".to_string();
    source.dispatch(Action::CommitCell {
        column: note,
        row: 0,
        text: multiline.clone(),
    });
    assert_eq!(cell(&source, note, 0), Cell::Text(Some(multiline.clone())));

    source.select_cell(sheet1, signal, 0);
    source.extend_selection(sheet1, note, 1);
    let text = source.copy_selection(sheet1).expect("non-empty selection");
    assert_eq!(
        text, "10\t\"line 1\nline 2\r\nTab\there \"\"quoted\"\"\"\n\t\n",
        "the text cell is quoted, the missing cell stays empty"
    );

    let dir2 = tempfile::tempdir().expect("temporary directory");
    let (_session2, mut target) = start_demo(dir2.path());
    let sheet1 = sheet_named(target.tree(), "Sheet1");
    let signal = column_id(&target, sheet1, "Signal");
    let note = column_id(&target, sheet1, "Note");
    target.select_cell(sheet1, signal, 2);
    target.dispatch(Action::Paste {
        sheet: sheet1,
        text,
    });
    assert_eq!(cell(&target, signal, 2), Cell::Float(10.0));
    assert_eq!(cell(&target, note, 2), Cell::Text(Some(multiline)));
    assert!(matches!(cell(&target, signal, 3), Cell::Float(v) if v.is_nan()));
    assert_eq!(
        cell(&target, note, 3),
        Cell::Text(None),
        "the next row received only its own cells"
    );
    assert_eq!(
        target
            .tree()
            .node(note)
            .and_then(|n| n.as_column())
            .map(|c| c.len()),
        Some(4),
        "two rows pasted (2 and 3), not one more: the line break \
         of the cell cut nothing"
    );
}

/// #259: a single multiline text cell copied then pasted elsewhere stays
/// a single cell (and not a row of cells).
#[test]
fn a_single_multiline_text_cell_pastes_as_one_cell() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut source) = start_demo(dir.path());
    let sheet1 = sheet_named(source.tree(), "Sheet1");
    let note = column_id(&source, sheet1, "Note");
    source.dispatch(Action::CommitCell {
        column: note,
        row: 0,
        text: "a\nb\tc".to_string(),
    });
    source.select_cell(sheet1, note, 0);
    let text = source.copy_selection(sheet1).expect("one cell");

    let dir2 = tempfile::tempdir().expect("temporary directory");
    let (_session2, mut target) = start_demo(dir2.path());
    let sheet1 = sheet_named(target.tree(), "Sheet1");
    let note = column_id(&target, sheet1, "Note");
    target.select_cell(sheet1, note, 1);
    target.dispatch(Action::Paste {
        sheet: sheet1,
        text,
    });
    assert_eq!(
        cell(&target, note, 1),
        Cell::Text(Some("a\nb\tc".to_string()))
    );
    assert_eq!(cell(&target, note, 0), Cell::Text(Some("a".to_string())));
}

/// #259: a non-breaking space typed in a text is not mistaken for a missing
/// cell on copy — it is copied and pasted back as is, whereas the cell
/// really missing stays an empty cell.
#[test]
fn a_typed_non_breaking_space_survives_copy_and_paste() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut source) = start_demo(dir.path());
    let sheet1 = sheet_named(source.tree(), "Sheet1");
    let note = column_id(&source, sheet1, "Note");
    let signal = column_id(&source, sheet1, "Signal");
    source.dispatch(Action::CommitCell {
        column: note,
        row: 0,
        text: "5\u{00A0}mm".to_string(),
    });
    source.dispatch(Action::CommitCell {
        column: note,
        row: 1,
        text: "m\u{00A0}s".to_string(),
    });
    assert_eq!(
        cell(&source, note, 1),
        Cell::Text(Some("m\u{00A0}s".to_string())),
        "typing a non-breaking space is kept by the model"
    );

    source.select_cell(sheet1, signal, 0);
    source.extend_selection(sheet1, note, 2);
    let text = source.copy_selection(sheet1).expect("non-empty selection");
    assert_eq!(
        text, "10\t5\u{00A0}mm\n\tm\u{00A0}s\n30\t\n",
        "typed non-breaking spaces are copied; the missing cell stays empty"
    );

    let dir2 = tempfile::tempdir().expect("temporary directory");
    let (_session2, mut target) = start_demo(dir2.path());
    let sheet1 = sheet_named(target.tree(), "Sheet1");
    let note = column_id(&target, sheet1, "Note");
    let signal = column_id(&target, sheet1, "Signal");
    target.select_cell(sheet1, signal, 0);
    target.dispatch(Action::Paste {
        sheet: sheet1,
        text,
    });
    assert_eq!(
        cell(&target, note, 0),
        Cell::Text(Some("5\u{00A0}mm".to_string()))
    );
    assert_eq!(
        cell(&target, note, 1),
        Cell::Text(Some("m\u{00A0}s".to_string()))
    );
    assert_eq!(
        cell(&target, note, 2),
        Cell::Text(None),
        "the cell that is really missing did not become a non-breaking space"
    );
}

/// A pure header-field copy/paste (selection entirely in the
/// header, touching no data row at all) goes onto the
/// fields of the target column — never onto its data.
#[test]
fn pure_header_field_copy_paste_writes_header_fields_not_data() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let time_col = column_id(&state, sheet1, "Time");
    let signal = column_id(&state, sheet1, "Signal");

    set_header_field(&mut state, time_col, HeaderField::LongName, "T");
    set_header_field(&mut state, time_col, HeaderField::Unit, "s");

    state.select_header_field(sheet1, time_col, HeaderField::LongName);
    state.extend_header_field_selection(sheet1, time_col, HeaderField::Unit);

    let text = state.copy_selection(sheet1).expect("non-empty selection");
    assert_eq!(text, "T\ns\n");

    let signal_data_before = [
        cell(&state, signal, 0),
        cell(&state, signal, 1),
        cell(&state, signal, 2),
    ];

    // Anchored on a header field of `Signal` (same kind as the copy):
    // the fields apply, the data do not move.
    state.select_header_field(sheet1, signal, HeaderField::LongName);
    state.dispatch(Action::Paste {
        sheet: sheet1,
        text,
    });

    let meta = state.column_meta(signal);
    assert_eq!(meta.long_name, "T");
    assert_eq!(meta.unit, "s");
    assert_eq!(cell(&state, signal, 0), signal_data_before[0]);
    assert_eq!(cell(&state, signal, 1), signal_data_before[1]);
    assert_eq!(cell(&state, signal, 2), signal_data_before[2]);
}

/// A header-field copy pasted onto a data cell pastes
/// nothing: the two kinds never mix (rule 2 of the behavior matrix,
/// see `stokesforge_app_state::state`) — the counterpart of
/// [`pure_header_field_copy_paste_writes_header_fields_not_data`] on the
/// anchor of the other kind.
#[test]
fn header_field_copy_pasted_onto_a_data_cell_anchor_pastes_nothing() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let time_col = column_id(&state, sheet1, "Time");
    let signal = column_id(&state, sheet1, "Signal");

    set_header_field(&mut state, time_col, HeaderField::LongName, "T");
    set_header_field(&mut state, time_col, HeaderField::Unit, "s");
    state.select_header_field(sheet1, time_col, HeaderField::LongName);
    state.extend_header_field_selection(sheet1, time_col, HeaderField::Unit);
    let text = state.copy_selection(sheet1).expect("non-empty selection");

    let signal_data_before = [cell(&state, signal, 0), cell(&state, signal, 1)];

    state.select_cell(sheet1, signal, 0);
    state.dispatch(Action::Paste {
        sheet: sheet1,
        text,
    });

    let meta = state.column_meta(signal);
    assert_eq!(meta.long_name, "", "nothing is pasted: different kinds");
    assert_eq!(meta.unit, "");
    assert_eq!(cell(&state, signal, 0), signal_data_before[0]);
    assert_eq!(cell(&state, signal, 1), signal_data_before[1]);
}

/// Reported bug: pasting a rectangle of several header fields onto a
/// single selected field must stretch the rectangle from that anchor —
/// like a reference spreadsheet — not fall back on the original fields
/// (which left all fields but the first empty).
#[test]
fn pasting_a_header_only_copy_onto_a_single_header_field_anchors_and_offsets_fields() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let time_col = column_id(&state, sheet1, "Time");
    let signal = column_id(&state, sheet1, "Signal");

    set_header_field(&mut state, time_col, HeaderField::LongName, "a");
    set_header_field(&mut state, time_col, HeaderField::Unit, "b");
    // "A": a valid column letter, because this text lands on the
    // Formula field of `Note` (formulas are entered as letters, #321).
    set_header_field(&mut state, time_col, HeaderField::Comment, "A");

    state.select_header_field(sheet1, time_col, HeaderField::LongName);
    state.extend_header_field_selection(sheet1, time_col, HeaderField::Comment);
    let text = state.copy_selection(sheet1).expect("non-empty selection");
    assert_eq!(text, "a\nb\nA\n");

    // Anchored on the Long Name field of `Signal` (the same rank as the first
    // copied field): the three fields apply, zero offset.
    state.select_header_field(sheet1, signal, HeaderField::LongName);
    state.dispatch(Action::Paste {
        sheet: sheet1,
        text: text.clone(),
    });
    let meta = state.column_meta(signal);
    assert_eq!(meta.long_name, "a");
    assert_eq!(meta.unit, "b");
    assert_eq!(meta.comment, "A");

    // Anchored on Unit this time (one rank lower): the rectangle
    // shifts by as much — Unit receives the first copied row, Comment
    // the second, and the third (which overflowed onto Formula) is written there
    // too since it stays within the 4 fields.
    let note = column_id(&state, sheet1, "Note");
    state.select_header_field(sheet1, note, HeaderField::Unit);
    state.dispatch(Action::Paste {
        sheet: sheet1,
        text,
    });
    let meta = state.column_meta(note);
    assert_eq!(meta.unit, "a");
    assert_eq!(meta.comment, "b");
    assert_eq!(formula_letters(&state, note).as_deref(), Some("A"));
}

// -- Formulas by identifier (#321) --

/// A letter that designates no column of the sheet is refused: nothing
/// is written, the status bar says which (on input as on paste).
#[test]
fn a_formula_naming_a_missing_column_is_refused_with_a_message() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let signal = column_id(&state, sheet1, "Signal");
    let note = column_id(&state, sheet1, "Note");

    set_header_field(&mut state, signal, HeaderField::Formula, "Z*2");
    assert_eq!(formula_letters(&state, signal), None, "nothing is written");
    let status = state.status().expect("a status");
    assert!(status.is_error(), "{}", status.text);
    assert!(
        status.text.contains("Column Z does not exist"),
        "unexpected status: {}",
        status.text
    );

    // On paste, the same refusal (and its own message): nothing is written.
    state.select_header_field(sheet1, note, HeaderField::Formula);
    state.dispatch(Action::Paste {
        sheet: sheet1,
        text: "Y*2".to_string(),
    });
    assert_eq!(formula_letters(&state, note), None, "paste refused");
    let status = state.status().expect("a status");
    assert!(status.is_error(), "{}", status.text);
    assert!(
        status.text.contains("Column Y does not exist"),
        "unexpected status: {}",
        status.text
    );
}

/// A read column that disappears leaves `#REF` in the displayed text; leaving
/// the cell without changing anything does not rewrite the formula (the displayed text is
/// never read back: `#REF` would be refused), and Undo gives back the column and its
/// letter.
#[test]
fn a_formula_reading_a_deleted_column_shows_ref_and_survives_an_unchanged_edit() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let time_col = column_id(&state, sheet1, "Time");
    let signal = column_id(&state, sheet1, "Signal");

    set_header_field(&mut state, signal, HeaderField::Formula, "A*2");
    let stored = state.column_meta(signal).formula;
    assert!(stored.is_some());

    state.select_column(sheet1, time_col);
    state.dispatch(Action::DeleteSelectedCells { sheet: sheet1 });
    assert_eq!(formula_letters(&state, signal).as_deref(), Some("#REF*2"));

    // Open the cell then leave it as is: the formula stays, with no
    // error message.
    state.begin_header_edit(signal, HeaderField::Formula);
    assert_eq!(
        state.header_editing_mut().map(|edit| edit.buffer.clone()),
        Some("#REF*2".to_string())
    );
    state.commit_header_edit();
    assert_eq!(state.column_meta(signal).formula, stored, "formula intact");
    assert!(
        !state.status().is_some_and(|status| status.is_error()),
        "no error: {}",
        status_text(&state)
    );
}

/// The refusal of the Formula cell is decided in `app-state`
/// (`header_edit_refusal`), which the grid paints: a column that reads
/// itself is refused as soon as it is typed, and committing a refused text writes nothing.
#[test]
fn a_formula_reading_its_own_column_is_refused_while_typing() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let note = column_id(&state, sheet1, "Note"); // column C

    state.begin_header_edit(note, HeaderField::Formula);
    assert_eq!(state.header_edit_refusal(), None, "nothing typed");
    state.header_editing_mut().expect("input").buffer = "C*2".to_string();
    let refusal = state.header_edit_refusal().expect("self-reading refused");
    assert!(
        refusal.contains("own column") && refusal.contains('C'),
        "unexpected sentence: {refusal}"
    );
    state.commit_header_edit();
    assert_eq!(formula_letters(&state, note), None, "nothing is written");

    // Lowercase is normalized: `a*2` is displayed as `A*2`.
    set_header_field(&mut state, note, HeaderField::Formula, "a*2");
    assert_eq!(formula_letters(&state, note).as_deref(), Some("A*2"));
}

/// #402: a column function that reads an X finds none paired with its column:
/// the Formula cell says so, until an X appears; a result that is entirely
/// empty because the functions found no point says so too, and a result with
/// values does not.
#[test]
fn the_formula_cell_says_why_a_column_function_has_no_value() {
    stokesforge_app_state::i18n::set_language(stokesforge_app_state::i18n::Language::English);
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let time_col = column_id(&state, sheet1, "Time");
    let note = column_id(&state, sheet1, "Note");

    // `at(B, 1)` reads the Signal at X = 1: no column has the role X yet.
    set_header_field(&mut state, note, HeaderField::Formula, "at(B, 1)");
    let issue = state.formula_issue(note).expect("no X paired with B");
    assert!(issue.contains("no X column"), "{issue}");

    // Time becomes the X: the frame goes, the formula computes.
    state.dispatch(Action::SetColumnRole {
        column: time_col,
        role: ColumnRole::X,
    });
    assert_eq!(state.formula_issue(note), None);

    // Nothing at X = 900: every cell is empty, the frame says why.
    set_header_field(&mut state, note, HeaderField::Formula, "at(B, 900)");
    let issue = state.formula_issue(note).expect("no point at 900");
    assert!(issue.contains("no point"), "{issue}");

    // A statistic needs no X: `max(B)` has a value on every row.
    set_header_field(&mut state, note, HeaderField::Formula, "max(B)");
    assert_eq!(state.formula_issue(note), None);
}

/// `#REF`: committing the displayed text without touching it writes nothing and is not
/// refused; modifying it (`#REF*3`) is refused; clearing erases; a
/// **syntax** error is never a refusal (it is saved). And the Formula cell
/// carries its problem (`formula_issue`) as long as it reads a deleted column.
#[test]
fn a_ref_formula_is_refused_only_when_its_text_is_changed() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let time_col = column_id(&state, sheet1, "Time");
    let signal = column_id(&state, sheet1, "Signal");

    set_header_field(&mut state, signal, HeaderField::Formula, "A*2");
    assert_eq!(state.formula_issue(signal), None, "no lost column");
    state.select_column(sheet1, time_col);
    state.dispatch(Action::DeleteSelectedCells { sheet: sheet1 });
    state.dispatch(Action::CloseColumnUse);
    let issue = state.formula_issue(signal).expect("a column is lost");
    assert!(issue.contains("Column deleted"), "{issue}");
    let stored = state.column_meta(signal).formula;

    state.begin_header_edit(signal, HeaderField::Formula);
    assert_eq!(state.header_edit_refusal(), None, "displayed text: valid");
    state.header_editing_mut().expect("input").buffer = "#REF*3".to_string();
    let refusal = state.header_edit_refusal().expect("modified #REF refused");
    assert!(refusal.contains("Column deleted"), "{refusal}");
    state.commit_header_edit();
    assert_eq!(
        state.column_meta(signal).formula,
        stored,
        "nothing is written"
    );

    state.begin_header_edit(signal, HeaderField::Formula);
    state.header_editing_mut().expect("input").buffer = "B*".to_string();
    assert_eq!(state.header_edit_refusal(), None, "a syntax error is saved");
    state.header_editing_mut().expect("input").buffer = String::new();
    assert_eq!(state.header_edit_refusal(), None, "clearing erases");
    state.cancel_header_edit();
}

/// Paste of a rectangle whose formula is refused in the destination
/// sheet: the other fields are pasted, and the status bar states the refusal
/// **last** (the same sentence as on input).
#[test]
fn a_refused_formula_in_a_paste_leaves_the_other_fields_pasted() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let sheet2 = sheet_named(state.tree(), "Sheet2");
    let signal = column_id(&state, sheet1, "Signal");
    let short = column_id(&state, sheet2, "Short"); // column A of Sheet2

    set_header_field(&mut state, signal, HeaderField::LongName, "ln");
    set_header_field(&mut state, signal, HeaderField::Unit, "u");
    set_header_field(&mut state, signal, HeaderField::Comment, "c");
    set_header_field(&mut state, signal, HeaderField::Formula, "A*2");
    // Two columns copied (Signal and Note) for a single destination: the overflow
    // notice is also issued, and the formula refusal must come after it.
    let note = column_id(&state, sheet1, "Note");
    state.select_header_field(sheet1, signal, HeaderField::LongName);
    state.extend_header_field_selection(sheet1, note, HeaderField::Formula);
    let text = state.copy_selection(sheet1).expect("non-empty selection");

    // In Sheet2, "A" is the receiving column: it would read itself.
    state.select_header_field(sheet2, short, HeaderField::LongName);
    state.dispatch(Action::Paste {
        sheet: sheet2,
        text,
    });
    let meta = state.column_meta(short);
    assert_eq!(
        (
            meta.long_name.as_str(),
            meta.unit.as_str(),
            meta.comment.as_str()
        ),
        ("ln", "u", "c"),
        "the other fields are pasted"
    );
    assert_eq!(formula_letters(&state, short), None, "formula refused");
    let status = state.status().expect("a status");
    assert!(status.is_error(), "{}", status.text);
    assert!(
        status.text.contains("own column"),
        "unexpected status (the refusal comes after the overflow notice): {}",
        status.text
    );
}

/// Ctrl+Shift+C (formulas) copies **letters**: pasted into another
/// sheet, they are re-read in the destination sheet (there, `A` is the
/// column of that sheet, not the one the copy comes from).
#[test]
fn copied_formulas_travel_as_letters_and_are_read_in_the_destination_sheet() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let sheet2 = sheet_named(state.tree(), "Sheet2");
    let signal = column_id(&state, sheet1, "Signal");
    let short = column_id(&state, sheet2, "Short");

    set_header_field(&mut state, signal, HeaderField::Formula, "A*2");
    state.select_column(sheet1, signal);
    assert_eq!(
        state.copy_full_columns(sheet1),
        None,
        "the question is asked"
    );
    state.dispatch(Action::ConfirmFullColumnsCopy(
        stokesforge_app_state::state::FullColumnsCopy::Formulas,
    ));
    let text = state.take_object_clipboard_text().expect("copied text");
    assert!(text.contains("A*2"), "letters: {text}");

    state.dispatch(Action::AddColumn { sheet: sheet2 });
    let columns = state.tree().children(sheet2).expect("sheet").to_vec();
    let arrival = *columns.last().expect("added column");
    assert_ne!(arrival, short);
    state.select_header_field(sheet2, arrival, HeaderField::XPrime);
    state.dispatch(Action::Paste {
        sheet: sheet2,
        text,
    });
    assert_eq!(formula_letters(&state, arrival).as_deref(), Some("A*2"));
    let lue = state
        .column_meta(arrival)
        .formula
        .expect("pasted formula")
        .columns();
    assert_eq!(
        lue,
        std::collections::BTreeSet::from([short]),
        "reads column A of Sheet2"
    );
}

// -- Removing a formula (#236) --

fn column_len(state: &AppState, column: NodeId) -> usize {
    state
        .tree()
        .node(column)
        .and_then(|node| node.as_column())
        .map_or(0, |c| c.len())
}

/// Clearing the Formula cell also deletes the column's values, and
/// Ctrl+Z gives back formula and data in one go (#236).
#[test]
fn removing_a_formula_deletes_the_values_and_one_undo_restores_both() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let time_col = column_id(&state, sheet1, "Time");
    let signal = column_id(&state, sheet1, "Signal");
    let len = column_len(&state, signal);
    assert!(len > 0);

    set_header_field(&mut state, signal, HeaderField::Formula, "A*2");
    let with_formula = column_len(&state, signal);
    let value = cell(&state, signal, 0);

    set_header_field(&mut state, signal, HeaderField::Formula, "");
    assert_eq!(formula_letters(&state, signal), None);
    assert_eq!(
        column_len(&state, signal),
        0,
        "the values go with the formula"
    );
    assert!(column_len(&state, time_col) > 0, "the other columns stay");

    state.dispatch(Action::Undo);
    assert_eq!(formula_letters(&state, signal).as_deref(), Some("A*2"));
    assert_eq!(column_len(&state, signal), with_formula);
    assert_eq!(cell(&state, signal, 0), value);
}

/// Clearing the field with the keyboard (Backspace) follows the same path.
#[test]
fn clearing_the_formula_field_with_backspace_also_deletes_the_values() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let signal = column_id(&state, sheet1, "Signal");
    set_header_field(&mut state, signal, HeaderField::Formula, "A*2");

    state.select_header_field(sheet1, signal, HeaderField::Formula);
    state.dispatch(Action::ClearSelectedCells { sheet: sheet1 });

    assert_eq!(formula_letters(&state, signal), None);
    assert_eq!(column_len(&state, signal), 0);
}

/// "Remove Formula, Keep Data" keeps the values; a single undo.
#[test]
fn remove_formula_keep_data_keeps_the_values_in_one_undo_step() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let signal = column_id(&state, sheet1, "Signal");
    set_header_field(&mut state, signal, HeaderField::Formula, "A*2");
    let len = column_len(&state, signal);
    let value = cell(&state, signal, 0);

    state.select_header_field(sheet1, signal, HeaderField::Formula);
    assert!(state.grid_menu(sheet1).unwrap().offers_keep_formula_data);
    state.dispatch(Action::RemoveFormulaKeepData { sheet: sheet1 });

    assert_eq!(formula_letters(&state, signal), None);
    assert_eq!(column_len(&state, signal), len);
    assert_eq!(cell(&state, signal, 0), value);
    assert!(!state.grid_menu(sheet1).unwrap().offers_keep_formula_data);

    state.dispatch(Action::Undo);
    assert_eq!(formula_letters(&state, signal).as_deref(), Some("A*2"));
}

/// Workbook › Columns: the same entry, on whole columns.
#[test]
fn remove_columns_formula_keep_data_works_from_the_columns_menu() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let time_col = column_id(&state, sheet1, "Time");
    let signal = column_id(&state, sheet1, "Signal");
    set_header_field(&mut state, signal, HeaderField::Formula, "A*2");
    let len = column_len(&state, signal);

    assert!(!state.any_formula(&[time_col]));
    assert!(state.any_formula(&[time_col, signal]));
    state.dispatch(Action::RemoveColumnsFormulaKeepData {
        columns: vec![time_col, signal],
    });
    assert_eq!(formula_letters(&state, signal), None);
    assert_eq!(column_len(&state, signal), len);
    state.dispatch(Action::Undo);
    assert_eq!(formula_letters(&state, signal).as_deref(), Some("A*2"));
}

/// The entry is offered only on an existing formula.
#[test]
fn keep_formula_data_is_only_offered_on_a_formula_field() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let signal = column_id(&state, sheet1, "Signal");

    state.select_header_field(sheet1, signal, HeaderField::Formula);
    assert!(!state.grid_menu(sheet1).unwrap().offers_keep_formula_data);
    state.select_header_field(sheet1, signal, HeaderField::Unit);
    assert!(!state.grid_menu(sheet1).unwrap().offers_keep_formula_data);
}

// -- Copy/paste behavior matrix (§3, see the head doc of
// `stokesforge_app_state::state`) --
//
// The tests above already cover several rows of the matrix (single
// cell → `Text`, relative rectangle of data or fields, overflow
// in columns, header/data crossing that pastes nothing). Those that
// follow cover the rest: external `Text` on any anchor,
// `Ctrl+Shift+C` (`AppState::copy_full_columns`), the other direction of the
// header/data crossing, and `Ctrl+Shift+V` (`Action::PasteAbsolute`) for a
// relative rectangle as for whole columns.

/// A single copied cell — here a header field — becomes `Text`: its
/// text alone. This is what makes it possible to copy a column's formula into
/// another's Formula field, the example that motivates the rule (see the head
/// doc of `stokesforge_app_state::state`).
#[test]
fn copying_a_single_formula_field_can_paste_it_into_another_columns_formula_field() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let signal = column_id(&state, sheet1, "Signal");

    // `Time` is column A: its formula cannot read itself,
    // so the copy source is `Signal`.
    set_header_field(&mut state, signal, HeaderField::Formula, "A*2");
    state.select_header_field(sheet1, signal, HeaderField::Formula);
    let text = state.copy_selection(sheet1).expect("non-empty selection");
    assert_eq!(text, "A*2", "a single copied cell: its text, nothing else");

    let note = column_id(&state, sheet1, "Note");
    state.select_header_field(sheet1, note, HeaderField::Formula);
    state.dispatch(Action::Paste {
        sheet: sheet1,
        text,
    });

    assert_eq!(formula_letters(&state, note).as_deref(), Some("A*2"));
}

/// Rule 1: an external text with no tab or line break (never
/// passed through `copy_selection`, hence never recognized as our
/// clipboard) is written into the anchored cell, whatever it is — header
/// field as well as data cell.
#[test]
fn external_plain_text_writes_into_whichever_anchor_is_selected() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let time_col = column_id(&state, sheet1, "Time");

    state.select_header_field(sheet1, time_col, HeaderField::LongName);
    state.dispatch(Action::Paste {
        sheet: sheet1,
        text: "Sensor 1".to_string(),
    });
    assert_eq!(state.column_meta(time_col).long_name, "Sensor 1");

    state.select_cell(sheet1, time_col, 0);
    state.dispatch(Action::Paste {
        sheet: sheet1,
        text: "42".to_string(),
    });
    assert_eq!(cell(&state, time_col, 0), Cell::Float(42.0));
}

/// #164: the x' row is edited like the other header fields, accepts a
/// number (decimal comma included), is undone in one step, and an empty text
/// clears it.
#[test]
fn x_prime_is_edited_cleared_and_undone_like_a_header_field() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let time_col = column_id(&state, sheet1, "Time");
    assert_eq!(state.column_meta(time_col).x_prime, None);

    set_header_field(&mut state, time_col, HeaderField::XPrime, " 12.5 ");
    assert_eq!(state.column_meta(time_col).x_prime, Some(12.5));

    state.dispatch(Action::Undo);
    assert_eq!(state.column_meta(time_col).x_prime, None);
    state.dispatch(Action::Redo);
    assert_eq!(state.column_meta(time_col).x_prime, Some(12.5));

    set_header_field(&mut state, time_col, HeaderField::XPrime, "");
    assert_eq!(state.column_meta(time_col).x_prime, None);
}

/// #164: a text that is not a finite number is refused with a message,
/// without writing anything or stacking any history.
#[test]
fn x_prime_refuses_text_nan_and_infinity() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let time_col = column_id(&state, sheet1, "Time");
    set_header_field(&mut state, time_col, HeaderField::XPrime, "3");
    for invalid in ["abc", "NaN", "inf", "-Infinity", "1e400"] {
        set_header_field(&mut state, time_col, HeaderField::XPrime, invalid);
        assert_eq!(
            state.column_meta(time_col).x_prime,
            Some(3.0),
            "{invalid:?} does not replace the value"
        );
        let status = state.status().expect("status message");
        assert!(status.is_error(), "{invalid:?} reported as an error");
    }
    // The refusals stacked nothing: a single Undo removes the "3".
    state.dispatch(Action::Undo);
    assert_eq!(state.column_meta(time_col).x_prime, None);
}

/// #164: x' is cleared with the selection (Delete) and pasted from one field to
/// another; pasting text onto x' is refused.
#[test]
fn x_prime_is_cleared_and_pasted_as_a_header_field() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let time_col = column_id(&state, sheet1, "Time");
    set_header_field(&mut state, time_col, HeaderField::XPrime, "7");
    state.select_header_field(sheet1, time_col, HeaderField::XPrime);
    state.dispatch(Action::ClearSelectedCells { sheet: sheet1 });
    assert_eq!(state.column_meta(time_col).x_prime, None);

    state.select_header_field(sheet1, time_col, HeaderField::XPrime);
    state.dispatch(Action::Paste {
        sheet: sheet1,
        text: "42".to_string(),
    });
    assert_eq!(state.column_meta(time_col).x_prime, Some(42.0));
    state.dispatch(Action::Paste {
        sheet: sheet1,
        text: "not a number".to_string(),
    });
    assert_eq!(state.column_meta(time_col).x_prime, Some(42.0));
}

/// Ctrl+Shift+C promotes to a whole column (header + body) the column
/// touched by a single-cell selection.
#[test]
fn copy_full_columns_promotes_the_touched_columns_from_a_single_cell_selection() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let time_col = column_id(&state, sheet1, "Time");

    set_header_field(&mut state, time_col, HeaderField::Unit, "s");
    state.select_cell(sheet1, time_col, 2);

    let text = state
        .copy_full_columns(sheet1)
        .expect("non-empty selection");
    // 5 header rows (x', Long Name, Unit, Comment, Formula) then the whole
    // body of `Time`, regardless of the selected cell.
    assert_eq!(text, "\n\ns\n\n\n0\n0.5\n1\n1.5\n2\n");
}

/// Symmetric to [`header_field_copy_pasted_onto_a_data_cell_anchor_pastes_nothing`]:
/// copied data rows, pasted onto a header field (different
/// kinds), paste nothing either.
#[test]
fn relative_data_rows_pasted_onto_a_header_field_anchor_pastes_nothing() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let time_col = column_id(&state, sheet1, "Time");
    let signal = column_id(&state, sheet1, "Signal");

    state.select_cell(sheet1, time_col, 0);
    state.extend_selection(sheet1, signal, 1);
    let text = state.copy_selection(sheet1).expect("non-empty selection");

    let unit_before = state.column_meta(time_col).unit;
    state.select_header_field(sheet1, time_col, HeaderField::Unit);
    state.dispatch(Action::Paste {
        sheet: sheet1,
        text,
    });

    assert_eq!(state.column_meta(time_col).unit, unit_before);
}

/// Ctrl+Shift+V on a relative rectangle of header fields ignores the anchor:
/// each field falls back on its original rank rather than shifting.
#[test]
fn paste_absolute_replays_relative_header_fields_on_their_original_rank() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let time_col = column_id(&state, sheet1, "Time");
    let signal = column_id(&state, sheet1, "Signal");

    set_header_field(&mut state, time_col, HeaderField::Unit, "s");
    set_header_field(&mut state, time_col, HeaderField::Comment, "x");
    state.select_header_field(sheet1, time_col, HeaderField::Unit);
    state.extend_header_field_selection(sheet1, time_col, HeaderField::Comment);
    state.copy_selection(sheet1).expect("non-empty selection");

    // Anchored on Long Name (above the original rank): in relative mode this
    // would shift everything up; Ctrl+Shift+V ignores the anchor.
    state.select_header_field(sheet1, signal, HeaderField::LongName);
    state.dispatch(Action::PasteAbsolute { sheet: sheet1 });

    let meta = state.column_meta(signal);
    assert_eq!(meta.long_name, "", "Long Name was not in the copy");
    assert_eq!(meta.unit, "s");
    assert_eq!(meta.comment, "x");
}

/// Ctrl+Shift+V on a relative rectangle of data ignores the anchor: each
/// row falls back on its original row rather than shifting — even
/// with an anchor chosen elsewhere.
#[test]
fn paste_absolute_replays_relative_data_rows_on_their_original_row() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let time_col = column_id(&state, sheet1, "Time");

    // Copy rows 2 and 3, then modify them: Ctrl+Shift+V must
    // restore them to their original rank, not from the chosen anchor
    // further down (row 0).
    state.select_cell(sheet1, time_col, 2);
    state.extend_selection(sheet1, time_col, 3);
    state.copy_selection(sheet1).expect("non-empty selection");
    state.dispatch(Action::CommitCell {
        column: time_col,
        row: 2,
        text: "99".to_string(),
    });
    state.dispatch(Action::CommitCell {
        column: time_col,
        row: 3,
        text: "98".to_string(),
    });

    state.select_cell(sheet1, time_col, 0);
    state.dispatch(Action::PasteAbsolute { sheet: sheet1 });

    assert_eq!(
        cell(&state, time_col, 0),
        Cell::Float(0.0),
        "the anchor (row 0) did not receive the first copied row"
    );
    assert_eq!(
        cell(&state, time_col, 2),
        Cell::Float(1.0),
        "restored to its original rank"
    );
    assert_eq!(cell(&state, time_col, 3), Cell::Float(1.5));
}

/// Ctrl+V of a whole-columns copy, anchored on a header field:
/// the fields from the anchor (included) apply as is, those
/// above are dropped; the body is pasted without offset.
#[test]
fn full_columns_pasted_onto_a_header_field_anchor_applies_from_that_field_down_with_an_unshifted_body(
) {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let time_col = column_id(&state, sheet1, "Time");
    let signal = column_id(&state, sheet1, "Signal");

    set_header_field(&mut state, time_col, HeaderField::LongName, "T");
    set_header_field(&mut state, time_col, HeaderField::Unit, "s");
    set_header_field(&mut state, time_col, HeaderField::Comment, "x");
    state.select_cell(sheet1, time_col, 0);
    let text = state
        .copy_full_columns(sheet1)
        .expect("non-empty selection");

    state.select_header_field(sheet1, signal, HeaderField::Unit);
    state.dispatch(Action::Paste {
        sheet: sheet1,
        text,
    });

    let meta = state.column_meta(signal);
    assert_eq!(meta.long_name, "", "above the anchor (Unit): dropped");
    assert_eq!(meta.unit, "s");
    assert_eq!(meta.comment, "x");
    assert_eq!(
        cell(&state, signal, 0),
        Cell::Float(0.0),
        "body pasted without offset"
    );
    assert_eq!(cell(&state, signal, 1), Cell::Float(0.5));
}

/// Ctrl+V of a whole-columns copy, anchored on a data
/// cell: the header is entirely dropped, the body shifts onto the anchor.
#[test]
fn full_columns_pasted_onto_a_data_cell_anchor_discards_header_and_shifts_the_body() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let time_col = column_id(&state, sheet1, "Time");
    let signal = column_id(&state, sheet1, "Signal");

    set_header_field(&mut state, time_col, HeaderField::LongName, "T");
    state.select_cell(sheet1, time_col, 0);
    let text = state
        .copy_full_columns(sheet1)
        .expect("non-empty selection");

    let signal_long_name_before = state.column_meta(signal).long_name;
    state.select_cell(sheet1, signal, 2);
    state.dispatch(Action::Paste {
        sheet: sheet1,
        text,
    });

    assert_eq!(
        state.column_meta(signal).long_name,
        signal_long_name_before,
        "the header is entirely dropped when the anchor is a data cell"
    );
    assert_eq!(
        cell(&state, signal, 2),
        Cell::Float(0.0),
        "1st copied row -> anchor row"
    );
    assert_eq!(cell(&state, signal, 3), Cell::Float(0.5));
    assert_eq!(
        cell(&state, signal, 0),
        Cell::Float(10.0),
        "before the anchor: unchanged"
    );
}

/// Ctrl+Shift+V of a whole-columns copy ignores the anchor in both
/// dimensions at once: the header applies all the same (unlike
/// Ctrl+V on a data anchor) and the body does not shift
/// (unlike Ctrl+V on a data anchor either).
#[test]
fn paste_absolute_of_full_columns_applies_everything_at_its_origin_regardless_of_anchor() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let time_col = column_id(&state, sheet1, "Time");
    let signal = column_id(&state, sheet1, "Signal");

    set_header_field(&mut state, time_col, HeaderField::LongName, "T");
    state.select_cell(sheet1, time_col, 0);
    state
        .copy_full_columns(sheet1)
        .expect("non-empty selection");

    state.select_cell(sheet1, signal, 2);
    state.dispatch(Action::PasteAbsolute { sheet: sheet1 });

    assert_eq!(
        state.column_meta(signal).long_name,
        "T",
        "the header applies even with a data anchor"
    );
    assert_eq!(
        cell(&state, signal, 0),
        Cell::Float(0.0),
        "body at its original rank"
    );
    assert_eq!(
        cell(&state, signal, 2),
        Cell::Float(1.0),
        "not shifted onto the anchor (row 2): receives Time[2], not Time[0]"
    );
}

/// A rectangle of fields shifted onto the anchor can overflow past Formula,
/// the last of the 4 fields: what overflows is **dropped** silently,
/// never wrapped onto the data nor overwritten onto Formula. A discreet
/// data-loss path, hence locked here.
#[test]
fn pasting_relative_header_fields_drops_what_overflows_past_the_formula_field() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let time_col = column_id(&state, sheet1, "Time");
    let signal = column_id(&state, sheet1, "Signal");

    set_header_field(&mut state, time_col, HeaderField::LongName, "a");
    // "A": lands on the Formula field of `Signal`, where only a letter
    // of a column of the sheet is valid (#321).
    set_header_field(&mut state, time_col, HeaderField::Unit, "A");
    set_header_field(&mut state, time_col, HeaderField::Comment, "c");
    set_header_field(&mut state, signal, HeaderField::LongName, "intact");
    set_header_field(&mut state, signal, HeaderField::Unit, "intact too");

    state.select_header_field(sheet1, time_col, HeaderField::LongName);
    state.extend_header_field_selection(sheet1, time_col, HeaderField::Comment);
    let text = state.copy_selection(sheet1).expect("non-empty selection");

    // Anchored on Comment: "a" lands there, "b" on Formula, "c"
    // would overflow past Formula.
    state.select_header_field(sheet1, signal, HeaderField::Comment);
    state.dispatch(Action::Paste {
        sheet: sheet1,
        text,
    });

    let meta = state.column_meta(signal);
    assert_eq!(meta.comment, "a");
    assert_eq!(formula_letters(&state, signal).as_deref(), Some("A"));
    assert_eq!(
        meta.long_name, "intact",
        "the fields above the anchor are not touched"
    );
    assert_eq!(meta.unit, "intact too");
    // "c" landed nowhere: neither in a field, nor in the neighboring
    // column (the copied rectangle is only one column wide).
    assert!(
        [&meta.long_name, &meta.unit, &meta.comment]
            .iter()
            .all(|value| *value != "c")
            && formula_letters(&state, signal).as_deref() != Some("c"),
        "\"c\" overflowed past Formula: it is dropped, not wrapped elsewhere"
    );
    let note = column_id(&state, sheet1, "Note");
    assert_eq!(
        state.column_meta(note).long_name,
        "",
        "the neighboring column is not touched either"
    );
}

/// A paste that touches several columns and several fields counts
/// for only **one** history entry: a single undo must put everything
/// back, not one per written field.
#[test]
fn pasting_full_columns_counts_as_a_single_history_entry() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let time_col = column_id(&state, sheet1, "Time");
    let signal = column_id(&state, sheet1, "Signal");
    let note = column_id(&state, sheet1, "Note");

    set_header_field(&mut state, time_col, HeaderField::LongName, "T");
    set_header_field(&mut state, time_col, HeaderField::Unit, "s");

    // Two whole columns copied (header + body).
    state.select_cell(sheet1, time_col, 0);
    state.extend_selection(sheet1, signal, 0);
    let text = state
        .copy_full_columns(sheet1)
        .expect("non-empty selection");

    let signal_before = state.column_meta(signal).clone();
    let note_before = state.column_meta(note).clone();
    let signal_data_before = cell(&state, signal, 0);
    let note_data_before = cell(&state, note, 0);

    // Pasted from `Signal`: overflows onto `Note`, and writes the 4
    // fields of each — far more than one operation.
    state.select_header_field(sheet1, signal, HeaderField::LongName);
    state.dispatch(Action::Paste {
        sheet: sheet1,
        text,
    });
    // `Signal` receives the fields of `Time`, and the overflow gives
    // `Note` those of `Signal`: several columns, several fields, plus
    // the two bodies — far more than one operation.
    assert_eq!(state.column_meta(signal).long_name, "T");
    assert_eq!(state.column_meta(signal).unit, "s");
    assert_eq!(cell(&state, signal, 0), Cell::Float(0.0));

    state.dispatch(Action::Undo);

    assert_eq!(
        state.column_meta(signal).long_name,
        signal_before.long_name,
        "a single undo must undo the whole paste"
    );
    assert_eq!(state.column_meta(signal).unit, signal_before.unit);
    assert_eq!(state.column_meta(note).long_name, note_before.long_name);
    assert_eq!(cell(&state, signal, 0), signal_data_before);
    assert_eq!(cell(&state, note, 0), note_data_before);
}

/// Ctrl+Shift+V replays the **internal** clipboard: without a prior copy
/// in this session, it has no original rank to replay. It is a
/// warning, not an error nor a panic.
#[test]
fn paste_absolute_without_a_prior_copy_is_a_notice_not_an_error() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let time_col = column_id(&state, sheet1, "Time");
    let before = state.snapshot().revision;

    state.select_cell(sheet1, time_col, 0);
    state.dispatch(Action::PasteAbsolute { sheet: sheet1 });

    let status = state.status().expect("a message");
    assert!(
        !status.is_error(),
        "\"nothing to paste\" is a normal state, not an error"
    );
    assert_eq!(state.snapshot().revision, before, "the model did not move");
}

/// Regression: Delete clears a header field, like Backspace. A
/// field is not repeated per row, so "clear and shift up" has
/// no other meaning than "clear" — and the key must not remain without
/// effect in the header.
#[test]
fn delete_on_a_header_field_selection_clears_it_like_backspace() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let time_col = column_id(&state, sheet1, "Time");

    set_header_field(&mut state, time_col, HeaderField::LongName, "T");
    set_header_field(&mut state, time_col, HeaderField::Unit, "s");

    // A single field selected.
    state.select_header_field(sheet1, time_col, HeaderField::LongName);
    state.dispatch(Action::DeleteSelectedCells { sheet: sheet1 });
    assert_eq!(state.column_meta(time_col).long_name, "");
    assert_eq!(
        state.column_meta(time_col).unit,
        "s",
        "only the selected field is cleared"
    );
    assert_eq!(
        cell(&state, time_col, 0),
        Cell::Float(0.0),
        "no data moved up"
    );

    // Range of fields: same rule.
    set_header_field(&mut state, time_col, HeaderField::LongName, "T");
    state.select_header_field(sheet1, time_col, HeaderField::LongName);
    state.extend_header_field_selection(sheet1, time_col, HeaderField::Unit);
    state.dispatch(Action::DeleteSelectedCells { sheet: sheet1 });
    assert_eq!(state.column_meta(time_col).long_name, "");
    assert_eq!(state.column_meta(time_col).unit, "");
    assert_eq!(cell(&state, time_col, 0), Cell::Float(0.0));
}

/// Two-stage Delete (`AppState::consume_delete_key`): the first
/// press on a selection clears the cell without shifting anything, exactly
/// like Backspace; the second press on *that same selection*
/// deletes for good and shifts what follows.
#[test]
fn pressing_delete_twice_on_the_same_selection_clears_then_shifts() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let time_col = column_id(&state, sheet1, "Time");

    // "Time" = [0.0, 0.5, 1.0, 1.5, 2.0].
    state.select_cell(sheet1, time_col, 2);

    // First press: clears, shifts nothing.
    let action = state
        .consume_delete_key(sheet1)
        .expect("a selection is active");
    assert_eq!(action, Action::ClearSelectedCells { sheet: sheet1 });
    state.dispatch(action);
    assert!(
        matches!(cell(&state, time_col, 2), Cell::Float(v) if v.is_nan()),
        "the cell is empty after the first press"
    );
    let time_len = state
        .tree()
        .get(time_col)
        .expect("column Time present")
        .as_column()
        .expect("column node")
        .len();
    assert_eq!(time_len, 5, "nothing was shifted on the first press");

    // Second press, same selection: deletes and shifts.
    let action = state
        .consume_delete_key(sheet1)
        .expect("the selection did not change");
    assert_eq!(action, Action::DeleteSelectedCells { sheet: sheet1 });
    state.dispatch(action);
    let time_len = state
        .tree()
        .get(time_col)
        .expect("column Time present")
        .as_column()
        .expect("column node")
        .len();
    assert_eq!(time_len, 4, "the second press does shift what follows");
    assert_eq!(cell(&state, time_col, 2), Cell::Float(1.5));
}

/// Changing the selection between the two presses restarts at the first stage
/// (clear): the second press deletes only if it targets *exactly*
/// the same selection as the previous one.
#[test]
fn changing_selection_between_two_delete_presses_restarts_at_clearing() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let time_col = column_id(&state, sheet1, "Time");

    state.select_cell(sheet1, time_col, 1);
    let action = state.consume_delete_key(sheet1).expect("active selection");
    assert_eq!(action, Action::ClearSelectedCells { sheet: sheet1 });
    state.dispatch(action);

    // The selection moves before the second press.
    state.select_cell(sheet1, time_col, 3);
    let action = state.consume_delete_key(sheet1).expect("active selection");
    assert_eq!(
        action,
        Action::ClearSelectedCells { sheet: sheet1 },
        "different selection: restart at the first stage, not a deletion"
    );
}

/// Another grid mutation (here Backspace) between the two
/// presses forgets the first, even if the selection did not move: a
/// following Delete press must not delete by surprise.
#[test]
fn another_grid_mutation_between_two_delete_presses_forgets_the_first() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let time_col = column_id(&state, sheet1, "Time");

    state.select_cell(sheet1, time_col, 1);
    let action = state.consume_delete_key(sheet1).expect("active selection");
    state.dispatch(action);

    // Another grid mutation slips in between the two presses,
    // on the same selection — as the caller (`app/grid.rs`) would before
    // dispatching `Action::ClearSelectedCells` for Backspace.
    state.forget_delete_repeat(sheet1);
    state.dispatch(Action::ClearSelectedCells { sheet: sheet1 });

    let action = state.consume_delete_key(sheet1).expect("active selection");
    assert_eq!(
        action,
        Action::ClearSelectedCells { sheet: sheet1 },
        "the interleaved mutation made it forget the first press"
    );
}

/// [`AppState::grid_menu`] — what the context menu offers is decided
/// here, without egui: the touched zone, the number agreement of the labels, and the
/// fact that "Delete and Shift Up" does not exist in the header, where Delete
/// has nothing to shift up (see [`AppState::grid_menu`] and the doc of
/// `GridMenu`).
#[test]
fn the_grid_menu_names_what_the_selection_actually_touches() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let time_col = column_id(&state, sheet1, "Time");
    let signal = column_id(&state, sheet1, "Signal");

    state.select_cell(sheet1, time_col, 1);
    let menu = state.grid_menu(sheet1).expect("a selected cell");
    assert_eq!(menu.zone, GridZone::Data);
    assert!(!menu.plural, "a single cell");
    assert!(!menu.multi_column);
    assert_eq!(menu.target_label(), "the cell");
    assert_eq!(menu.copy_full_columns_label(), "Copy entire column");
    assert!(
        menu.offers_delete_shift,
        "Delete shifts what follows up in the data"
    );

    // Several rows of a single column: plural in cells, but still
    // a single whole column to promote.
    state.extend_selection(sheet1, time_col, 3);
    let menu = state.grid_menu(sheet1).expect("a selected rectangle");
    assert!(menu.plural);
    assert!(
        !menu.multi_column,
        "three stacked cells touch only one column"
    );
    assert_eq!(menu.target_label(), "the cells");
    assert_eq!(menu.copy_full_columns_label(), "Copy entire column");

    state.extend_selection(sheet1, signal, 3);
    let menu = state.grid_menu(sheet1).expect("a selected rectangle");
    assert!(menu.multi_column);
    assert_eq!(menu.copy_full_columns_label(), "Copy entire columns");

    // Header: a field is not repeated per row, nothing to shift up.
    state.select_header_field(sheet1, time_col, HeaderField::LongName);
    let menu = state.grid_menu(sheet1).expect("a selected field");
    assert_eq!(menu.zone, GridZone::Header);
    assert_eq!(menu.target_label(), "the field");
    assert!(
        !menu.offers_delete_shift,
        "in the header, Delete only clears: two entries for the same effect would make no sense"
    );
    state.extend_header_field_selection(sheet1, time_col, HeaderField::Unit);
    assert_eq!(
        state
            .grid_menu(sheet1)
            .expect("a range of fields")
            .target_label(),
        "the fields"
    );

    // Whole columns: a single column stays singular.
    state.select_column(sheet1, time_col);
    let menu = state.grid_menu(sheet1).expect("a selected column");
    assert_eq!(menu.zone, GridZone::Columns);
    assert_eq!(menu.target_label(), "the column");
    assert!(menu.offers_delete_shift);
    state.extend_column_selection(sheet1, signal);
    assert_eq!(
        state
            .grid_menu(sheet1)
            .expect("a range of columns")
            .target_label(),
        "the columns"
    );
}

/// "Paste Absolute" (Ctrl+Shift+V) replays the **internal** clipboard:
/// the menu must grey it out as long as nothing was copied from the grid in
/// this session, rather than offering an entry that would only set a
/// notice (lock for the menu counterpart of
/// [`paste_absolute_without_a_prior_copy_is_a_notice_not_an_error`]).
#[test]
fn the_grid_menu_greys_out_paste_absolute_until_something_was_copied() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let time_col = column_id(&state, sheet1, "Time");

    state.select_cell(sheet1, time_col, 0);
    assert!(
        !state
            .grid_menu(sheet1)
            .expect("a selected cell")
            .paste_absolute_enabled,
        "nothing has been copied from the grid yet"
    );

    state.copy_selection(sheet1).expect("copy of a cell");
    assert!(
        state
            .grid_menu(sheet1)
            .expect("a selected cell")
            .paste_absolute_enabled,
        "a copy from the grid gives something to replay"
    );
}

/// Without a selection, no menu to open — the caller must abstain rather
/// than show a menu where every entry would have no effect.
#[test]
fn the_grid_menu_is_absent_without_a_selection() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");

    assert!(state.grid_menu(sheet1).is_none());
}

// -- 13. The Python script — what must hold WITHOUT the `python` feature --
//
// What requires an interpreter (execution, output, undoing a script
// write, lazy startup proven at runtime) lives in
// `tests/script_panel.rs`, which compiles only with the feature. Here, the proof
// that the interpreter **never** starts in this configuration is not
// a separate test: it is **structural**. `Cargo.toml` declares
// `stokesforge-script` and `stokesforge-python` as optional dependencies enabled
// only by the `python` feature of this crate; without it, neither
// is compiled into the binary — so there is literally no code
// able to start CPython, however `AppState` is
// built or used. `cargo tree -p stokesforge-app-state` (without `--features
// python`) confirms it: neither `stokesforge-python` nor `stokesforge-script`
// appears there.
//
// The two tests that follow assume the feature is ABSENT ("without the Python
// bridge, RunScript is refused"); they are therefore `#[cfg(not(feature =
// "python"))]` — otherwise, compiled with the feature (`cargo test --features
// python`), `script_available()` would be true and they would rightly
// fail. The "with the feature" counterpart of these criteria lives in
// `tests/script_panel.rs`.

#[cfg(not(feature = "python"))]
#[test]
fn script_available_is_false_and_run_script_explains_why_without_the_feature() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let before = state.snapshot().revision;

    assert!(
        !state.script_available(),
        "the Python bridge is not compiled in this configuration"
    );
    assert!(state.script_output().is_none(), "nothing has run yet");

    state.dispatch(Action::RunScript);

    let status = state.status().expect("an explicit message");
    assert!(
        status.is_error(),
        "being unable to run a script is not a silent success"
    );
    assert_eq!(
        status.text,
        stokesforge_app_state::tr!("error-script-unavailable"),
        "the message explains why: {}",
        status.text
    );
    assert_eq!(
        state.snapshot().revision,
        before,
        "no command was sent to the actor"
    );
    assert!(
        !state.can_undo(),
        "nothing to undo: nothing was run by the script"
    );
    assert!(
        state.script_output().is_none(),
        "no output: the refused script never ran"
    );
}

#[cfg(not(feature = "python"))]
#[test]
fn interrupt_script_is_also_explained_without_the_feature() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());

    state.dispatch(Action::InterruptScript);

    let status = state.status().expect("an explicit message");
    assert!(status.is_error());
    assert_eq!(
        status.text,
        stokesforge_app_state::tr!("error-script-unavailable")
    );
}

#[test]
fn script_source_lives_in_state_and_is_not_lost_by_other_actions() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");

    assert_eq!(state.script_source(), "", "empty by default");

    state.set_script_source("print('hello')".to_string());
    assert_eq!(state.script_source(), "print('hello')");

    // Actions unrelated to the script must not
    // touch it: the script buffer is independent of the rest of the state.
    state.dispatch(Action::AddColumn { sheet: sheet1 });
    assert_eq!(state.script_source(), "print('hello')");
    state.dispatch(Action::Undo);
    assert_eq!(state.script_source(), "print('hello')");

    state.set_script_source("v = 2".to_string());
    assert_eq!(state.script_source(), "v = 2", "re-read after replacement");
}

/// Phase R4: the bench must know where the graphs are, since it displays
/// one. The counterpart of `sheets()`, and the only part of the rendering panel that
/// can be tested without a window — the rest is covered by
/// `stokesforge-render-egui` (`tests/cache_session.rs`).
#[test]
fn the_project_graphs_are_listed_with_their_path() {
    let dir = tempfile::tempdir().expect("temporary folder");
    let mut project = demo_project();
    let root = project.root();
    let sheet = sheet_named(project.tree(), "Sheet1");
    let column = project
        .tree()
        .child_by_name(sheet, "Time")
        .expect("Sheet1 readable")
        .expect("column Time");
    let mut panel = stokesforge_plot::PanelSpec::at(0, 0);
    panel.series.push(stokesforge_plot::SeriesSpec::new(column));
    let spec = stokesforge_plot::PlotSpec::empty("Test").with_panels(panel);
    project
        .add_graph(root, Some("Graph1"), spec)
        .expect("graph");

    let (session, state) = start(project, &dir.path().join("project.stf"));
    let graphs = state.graphs();
    assert_eq!(graphs.len(), 1, "a single graph: {graphs:?}");
    assert_eq!(graphs[0].1, "Graph1");
    // A sheet is not a graph, and conversely.
    assert!(state.sheets().iter().all(|(id, _)| *id != graphs[0].0));
    unused(&session.handle());
}

// -- Miscellaneous: the Project Manager panel --

/// The Project Manager stops at `Workbook`/`Graph`/`Matrix`/`Note`: never
/// descending into the sheets of a workbook nor into the content of a
/// graph (§1 of the task). `books()` remains the flat counterpart of `graphs()`,
/// used by the initial layout of `stokesforge_app::app::StokesForgeApp::new`.
#[test]
fn books_are_enumerated_like_graphs_with_their_readable_path() {
    let dir = tempfile::tempdir().expect("temporary folder");
    let (_session, state) = start_demo(dir.path());

    let books = state.books();
    assert_eq!(books.len(), 1, "a single workbook: {books:?}");
    assert_eq!(books[0].1, "Book1");
}

/// [`AppState::project_tree`] shows the folders and the four kinds of
/// window, never the sheets of a workbook.
#[test]
fn project_tree_stops_at_windows_and_never_descends_into_a_book() {
    use stokesforge_app_state::state::{ProjectTreeNode, WindowKind};

    let dir = tempfile::tempdir().expect("temporary folder");
    let mut project = demo_project();
    let root = project.root();
    let folder = project
        .add_folder(root, "Experiments")
        .expect("folder Experiments");
    project
        .add_note(folder, Some("Note1"), "some text")
        .expect("note");

    let (_session, state) = start(project, &dir.path().join("project.stf"));
    let tree = state.project_tree();

    let book = tree
        .iter()
        .find(|node| {
            matches!(
                node,
                ProjectTreeNode::Window { name, kind, .. }
                    if name == "Book1" && *kind == WindowKind::Book
            )
        })
        .expect("Book1 is a window of the Project Manager");
    // A window of the Project Manager never has children in this view,
    // even if `Workbook` does have `Worksheet`s in the model.
    assert!(matches!(book, ProjectTreeNode::Window { .. }));

    let folder_node = tree
        .iter()
        .find_map(|node| match node {
            ProjectTreeNode::Folder { name, children, .. } if name == "Experiments" => {
                Some(children)
            }
            _ => None,
        })
        .expect("Experiments is an unfolded folder in the Project Manager");
    assert!(
        folder_node.iter().any(|child| matches!(
            child,
            ProjectTreeNode::Window {
                kind: WindowKind::Note,
                ..
            }
        )),
        "the note appears in its folder"
    );
}

// -- Miscellaneous: note editing --

/// The detached note edits on a local view: its input does not reach the state
/// of the root, and its validation joins the undoable command path.
#[test]
fn detached_note_view_keeps_its_buffer_local_and_commits_through_the_root() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let mut project = demo_project();
    let root = project.root();
    let note = project
        .add_note(root, Some("Note1"), "before")
        .expect("note");
    let (_session, mut state) = start(project, &dir.path().join("project.stf"));

    let mut detached = state.fork_for_detached_view().unwrap();
    detached.begin_note_edit(note);
    if let Some(edit) = detached.note_editing_mut() {
        edit.buffer = "after".to_string();
    }
    assert!(state.note_editing().is_none());
    assert_eq!(state.note_text(note), Some("before".to_string()));

    state.dispatch(Action::SetNoteText {
        note,
        text: "after".to_string(),
    });
    detached.sync_detached_snapshot(state.snapshot().clone());
    assert_eq!(detached.note_text(note), Some("after".to_string()));
    assert!(state.can_undo());
    state.dispatch(Action::Undo);
    assert_eq!(state.note_text(note), Some("before".to_string()));
}

#[test]
fn editing_a_note_commits_on_dispatch_and_is_undoable() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let mut project = demo_project();
    let root = project.root();
    let note = project
        .add_note(root, Some("Note1"), "before")
        .expect("note");

    let (_session, mut state) = start(project, &dir.path().join("project.stf"));
    assert_eq!(state.note_text(note), Some("before".to_string()));

    state.begin_note_edit(note);
    if let Some(edit) = state.note_editing_mut() {
        edit.buffer = "after".to_string();
    }
    state.commit_note_edit();

    assert_eq!(state.note_text(note), Some("after".to_string()));
    assert!(state.editing().is_none());
    assert!(state.can_undo());

    state.dispatch(Action::Undo);
    assert_eq!(state.note_text(note), Some("before".to_string()));
}

// -- 15. Delete on a cell selection --

#[test]
fn deleting_a_single_selected_row_shifts_the_rest_up_instead_of_truncating() {
    // Regression: selecting the 3rd row (index 2, 0-indexed) of a
    // column and pressing Delete must remove *that single row* and shift
    // the following ones up by one position — not truncate everything after
    // the selected row.
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let time_col = column_id(&state, sheet1, "Time");

    // "Time" = [0.0, 0.5, 1.0, 1.5, 2.0] before the deletion.
    assert_eq!(state.row_count(sheet1), 5);

    state.select_cell(sheet1, time_col, 2);
    state.dispatch(Action::DeleteSelectedCells { sheet: sheet1 });

    // Only the row at index 2 (value 1.0) disappeared from the selected
    // column; rows 3 and 4 (1.5, 2.0) moved up to indexes 2
    // and 3 — the column loses exactly one value, it is not truncated
    // at its end. `state.row_count` does not fit here: it is the maximum of the
    // column lengths of the sheet, and "Signal" (not selected)
    // keeps its 5 rows.
    let time_len = state
        .tree()
        .get(time_col)
        .expect("column Time present")
        .as_column()
        .expect("column node")
        .len();
    assert_eq!(time_len, 4);
    let values: Vec<Cell> = (0..4).map(|row| cell(&state, time_col, row)).collect();
    assert_eq!(
        values,
        vec![
            Cell::Float(0.0),
            Cell::Float(0.5),
            Cell::Float(1.5),
            Cell::Float(2.0),
        ]
    );

    assert!(state.can_undo());
    state.dispatch(Action::Undo);
    let temps_len_after_undo = state
        .tree()
        .get(time_col)
        .expect("column Time present")
        .as_column()
        .expect("column node")
        .len();
    assert_eq!(temps_len_after_undo, 5);
    assert_eq!(cell(&state, time_col, 2), Cell::Float(1.0));
}

#[test]
fn committing_a_note_without_typing_writes_nothing() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let mut project = demo_project();
    let root = project.root();
    let note = project
        .add_note(root, Some("Note1"), "unchanged")
        .expect("note");

    let (_session, mut state) = start(project, &dir.path().join("projet.stf"));
    let revision = state.snapshot().revision;

    state.begin_note_edit(note);
    state.commit_note_edit();

    assert_eq!(state.snapshot().revision, revision, "nothing was written");
    assert!(!state.can_undo());
}

/// Importing several files creates one sheet per file, each named
/// after its file (`file_stem`) and carrying the file's columns —
/// and the whole batch fits in **one** history entry: a single
/// Ctrl+Z (`AppState::dispatch(Action::Undo)`) removes both sheets
/// at once, not one at a time.
#[test]
fn importing_several_files_creates_one_sheet_each_in_a_single_undo_entry() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let book = book_named(state.tree(), "Book1");
    let sheets_before = state.worksheets_of(book).len();

    let file_a = dir.path().join("measure_a.csv");
    std::fs::write(&file_a, "X,Y\n1,10\n2,20\n").expect("wrote measure_a.csv");
    let file_b = dir.path().join("measure_b.csv");
    std::fs::write(&file_b, "X,Y\n3,30\n4,40\n").expect("wrote measure_b.csv");

    let mut recipe = stokesforge_import::ImportRecipe::new("Untitled");
    // Both files have a header row "X,Y": without this, it is
    // read as a data row (two missing values) and the column names
    // fall back on the file name rather than "X"/"Y".
    recipe.has_column_header_row = true;
    state.dispatch(Action::ImportFiles {
        destination: ImportDestination::ActiveBook(book),
        recipe,
        paths: vec![file_a.clone(), file_b.clone()],
    });

    let status = state.status().expect("a status message");
    assert!(
        !status.is_error(),
        "the import must not be reported as a failure"
    );
    assert!(
        status.text.contains("2"),
        "the message sums up the number of imported files: {}",
        status.text
    );

    let tree = state.tree();
    assert_eq!(
        state.worksheets_of(book).len(),
        sheets_before + 2,
        "one sheet per imported file"
    );
    let sheet_a = sheet_named(tree, "measure_a");
    let sheet_b = sheet_named(tree, "measure_b");
    let columns_a = state.visible_columns(sheet_a);
    assert_eq!(columns_a.len(), 2, "two columns read in measure_a.csv");
    assert_eq!(state.column_meta(columns_a[0].0).long_name, "X");
    assert_eq!(state.column_meta(columns_a[1].0).long_name, "Y");
    assert_eq!(state.column_meta(columns_a[0].0).role, ColumnRole::X);
    assert_eq!(state.column_meta(columns_a[1].0).role, ColumnRole::Y);
    assert_eq!(cell_f64(&state, columns_a[0].0, 0), 1.0);
    assert_eq!(cell_f64(&state, columns_a[1].0, 1), 20.0);
    let columns_b = state.visible_columns(sheet_b);
    assert_eq!(state.column_meta(columns_b[0].0).role, ColumnRole::X);
    assert_eq!(state.column_meta(columns_b[1].0).role, ColumnRole::Y);
    assert_eq!(cell_f64(&state, columns_b[0].0, 0), 3.0);

    // A single history entry for the whole batch: a single `Undo`
    // removes both sheets at once.
    assert!(state.can_undo());
    state.dispatch(Action::Undo);
    assert_eq!(
        state.worksheets_of(book).len(),
        sheets_before,
        "a single Ctrl+Z removes the whole import, sheets of both files included"
    );
    assert!(
        !state.can_undo(),
        "the whole import counted for only one history entry"
    );

    // Redo also restores the roles, for each file of the batch
    // (issue #3), with the same column identifiers and their data.
    state.dispatch(Action::Redo);
    assert_eq!(state.worksheets_of(book).len(), sheets_before + 2);
    for (sheet, columns, first_x) in [(sheet_a, columns_a, 1.0), (sheet_b, columns_b, 3.0)] {
        assert_eq!(state.visible_columns(sheet), columns);
        assert_eq!(state.column_meta(columns[0].0).role, ColumnRole::X);
        assert_eq!(state.column_meta(columns[1].0).role, ColumnRole::Y);
        assert_eq!(cell_f64(&state, columns[0].0, 0), first_x);
    }
}

/// The two destinations that create workbooks keep the rule
/// "one sheet per file" and the whole batch stays undoable at once.
#[test]
fn importing_into_new_books_groups_files_as_requested() {
    for destination in [
        ImportDestination::NewBookPerFile,
        ImportDestination::NewBookForAll,
    ] {
        let dir = tempfile::tempdir().expect("temporary directory");
        let (_session, mut state) = start_demo(dir.path());
        let root = state.tree().root();
        let before = state.tree().node(root).expect("root").children().len();
        let a = dir.path().join("measure_a.csv");
        let b = dir.path().join("measure_b.csv");
        std::fs::write(&a, "1,10\n2,20\n").unwrap();
        std::fs::write(&b, "3,30\n4,40\n").unwrap();
        state.dispatch(Action::ImportFiles {
            destination,
            recipe: ImportRecipe::new("Untitled"),
            paths: vec![a, b],
        });
        let added = state.tree().node(root).expect("root").children().len() - before;
        let expected = if destination == ImportDestination::NewBookPerFile {
            2
        } else {
            1
        };
        assert_eq!(added, expected);
        let sheet_a = sheet_named(state.tree(), "measure_a");
        let sheet_b = sheet_named(state.tree(), "measure_b");
        let book_a = state.tree().node(sheet_a).unwrap().parent().unwrap();
        let book_b = state.tree().node(sheet_b).unwrap().parent().unwrap();
        assert_eq!(
            book_a == book_b,
            destination == ImportDestination::NewBookForAll
        );
        assert_eq!(
            state.worksheets_of(book_a).len(),
            if expected == 1 { 2 } else { 1 }
        );
        assert_eq!(
            state.worksheets_of(book_b).len(),
            if expected == 1 { 2 } else { 1 }
        );
        assert_eq!(
            cell_f64(&state, state.visible_columns(sheet_b)[0].0, 0),
            3.0
        );
        state.dispatch(Action::Undo);
        assert_eq!(
            state.tree().node(root).expect("root").children().len(),
            before
        );
    }
}

/// The detached view keeps its selections and its copies, then provides this
/// context to the main command path before a deferred paste.
#[test]
fn detached_book_view_reuses_the_grid_selection_and_clipboard() {
    let dir = tempfile::tempdir().unwrap();
    let (_session, mut state) = start_demo(dir.path());
    let book = book_named(state.tree(), "Book1");
    let sheet = sheet_named(state.tree(), "Sheet1");
    let columns = state.visible_columns(sheet);
    let source = columns[1].0;
    let target = columns[0].0;
    let mut detached = state.fork_for_detached_view().unwrap();
    detached.select_cell(sheet, source, 0);
    let copied = detached.copy_selection(sheet).unwrap();
    state.adopt_detached_book_view(&mut detached, book);
    assert_eq!(state.selection(sheet), detached.selection(sheet));
    state.select_cell(sheet, target, 1);
    state.dispatch(Action::Paste {
        sheet,
        text: copied,
    });
    assert_eq!(cell_f64(&state, target, 1), cell_f64(&state, source, 0));
}

/// An unreadable file produces no empty workbook; a later valid file
/// of the same batch is still imported.
#[test]
fn new_book_import_creates_nothing_for_an_unreadable_file() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let root = state.tree().root();
    let before = state.tree().node(root).unwrap().children().len();
    let valid = dir.path().join("valid.csv");
    std::fs::write(&valid, "1,2\n").unwrap();
    state.dispatch(Action::ImportFiles {
        destination: ImportDestination::NewBookPerFile,
        recipe: ImportRecipe::new("Untitled"),
        paths: vec![dir.path().join("absent.csv"), valid],
    });
    assert_eq!(
        state.tree().node(root).unwrap().children().len(),
        before + 1
    );
    assert!(state.status().unwrap().is_error());
}

#[test]
fn import_without_an_active_book_defaults_to_one_new_book_for_the_batch() {
    let _recipes = isolate_recipe_store();
    let dir = tempfile::tempdir().unwrap();
    let (_session, mut state) = start_demo(dir.path());
    state.open_import(None::<NodeId>);
    let import = state.import_state().unwrap();
    assert_eq!(import.active_book, None);
    assert_eq!(import.destination, ImportDestination::NewBookForAll);
}

/// An unreadable file does not interrupt the batch: the following files are
/// imported all the same, and the final message sums up how many
/// actually succeeded.
#[test]
fn a_file_read_failure_does_not_abort_the_rest_of_the_batch() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let book = book_named(state.tree(), "Book1");
    let sheets_before = state.worksheets_of(book).len();

    let missing = dir.path().join("absent.csv");
    let file_ok = dir.path().join("measure.csv");
    std::fs::write(&file_ok, "X,Y\n1,10\n").expect("wrote measure.csv");

    let recipe = stokesforge_import::ImportRecipe::new("Untitled");
    state.dispatch(Action::ImportFiles {
        destination: ImportDestination::ActiveBook(book),
        recipe,
        paths: vec![missing, file_ok],
    });

    assert_eq!(
        state.worksheets_of(book).len(),
        sheets_before + 1,
        "only the readable file produced a sheet"
    );
    let status = state.status().expect("a summary status message");
    assert!(
        status.text.contains('1'),
        "the final summary counts 1 file actually imported: {}",
        status.text
    );
    assert!(state.can_undo());
    state.dispatch(Action::Undo);
    assert_eq!(state.worksheets_of(book).len(), sheets_before);
}

/// `AppState::open_import` starts on the first saved template if there is
/// one, otherwise on a permissive recipe — the two branches of its doc.
#[test]
fn opening_import_starts_from_the_first_saved_recipe_or_a_permissive_default() {
    let _isolation = isolate_recipe_store();
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let book = book_named(state.tree(), "Book1");

    // Nothing saved: permissive recipe by default.
    state.open_import(book);
    {
        let import = state.import_state().expect("import window open");
        assert_eq!(import.recipe.separator, Separator::Auto);
        assert_eq!(import.recipe.decimal, DecimalStyle::Dot);
        assert_eq!(import.recipe.header, HeaderSkip::None);
        assert_eq!(import.recipe.footer_skip_lines, 0);
        assert!(!import.recipe.has_column_header_row);
        assert_eq!(import.recipe.name_source, NameSource::Filename);
    }
    state.cancel_import();

    // A template already saved on disk (written directly, without going
    // through the window): `open_import` must offer it right away.
    let mut store = RecipeStore::default();
    let mut jasco = ImportRecipe::new("Jasco UV-Vis");
    jasco.separator = Separator::Semicolon;
    jasco.decimal = DecimalStyle::Comma;
    store.upsert(jasco);
    store.save().expect("writes the isolated template to disk");

    state.open_import(book);
    let import = state.import_state().expect("import window open");
    assert_eq!(import.recipe.name, "Jasco UV-Vis");
    assert_eq!(import.recipe.separator, Separator::Semicolon);
    assert_eq!(import.recipe.decimal, DecimalStyle::Comma);
}

/// S-T1 (#224): during typing, the import previews keep the previous
/// recipe until 0.3 s without change; outside typing, a change is
/// immediate.
#[test]
fn the_import_preview_waits_for_a_typing_pause_but_not_for_other_controls() {
    let _isolation = isolate_recipe_store();
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let book = book_named(state.tree(), "Book1");
    state.open_import(book);
    let import = state.import_state_mut().expect("import window open");
    let before = import.recipe.comment_prefix.clone();
    assert_eq!(import.debounce_preview(true, 10.0), None, "nothing changed");

    // A keystroke: the preview stays on the old recipe.
    import.recipe.comment_prefix = "#".to_string();
    let wait = import
        .debounce_preview(true, 10.0)
        .expect("a pause is running");
    assert!((wait - 0.3).abs() < 1e-9);
    assert_eq!(import.preview_recipe().comment_prefix, before);

    // Another keystroke pushes back the deadline.
    import.recipe.comment_prefix = "##".to_string();
    assert!(import.debounce_preview(true, 10.2).is_some());
    assert_eq!(import.preview_recipe().comment_prefix, before);
    assert!(import.debounce_preview(true, 10.4).is_some(), "0.2 s only");

    // Once the pause has elapsed, the preview follows the recipe.
    assert_eq!(import.debounce_preview(true, 10.55), None);
    assert_eq!(import.preview_recipe().comment_prefix, "##");

    // Outside typing (list, checkbox): immediate.
    import.recipe.comment_prefix = ";".to_string();
    assert_eq!(import.debounce_preview(false, 11.0), None);
    assert_eq!(import.preview_recipe().comment_prefix, ";");
}

/// "New Template" starts from a permissive recipe again, even after
/// loading/modifying an existing template.
#[test]
fn new_import_recipe_resets_to_a_permissive_default() {
    let _isolation = isolate_recipe_store();
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let book = book_named(state.tree(), "Book1");

    state.open_import(book);
    {
        let import = state.import_state_mut().expect("import window open");
        import.recipe.separator = Separator::Tab;
        import.recipe.decimal = DecimalStyle::Comma;
        import.recipe.footer_skip_lines = 5;
        import.recipe.has_column_header_row = true;
        import.recipe.name = "Something else".to_string();
    }

    state.new_import_recipe();
    let import = state.import_state().expect("import window open");
    assert_eq!(
        import.recipe.name,
        stokesforge_app_state::tr!("import-template-untitled")
    );
    assert_eq!(import.recipe.separator, Separator::Auto);
    assert_eq!(import.recipe.decimal, DecimalStyle::Dot);
    assert_eq!(import.recipe.header, HeaderSkip::None);
    assert_eq!(import.recipe.footer_skip_lines, 0);
    assert!(!import.recipe.has_column_header_row);
    assert_eq!(import.recipe.name_source, NameSource::Filename);
}

/// The template dropdown loads the chosen template into the recipe
/// being edited; an unknown name has no effect (no panic, no
/// half-replaced recipe).
#[test]
fn select_import_recipe_loads_a_saved_model_and_ignores_unknown_names() {
    // `select_import_recipe` itself only searches the in-memory `Vec`,
    // but `open_import` just below does call
    // `RecipeStore::load()` for real: without isolation, it would read
    // `XDG_CONFIG_HOME` as an *other* test of this file, running in
    // parallel and isolated in its own temporary directory, would
    // have left it at that precise moment — and would load its template instead of
    // nothing, before the next line overwrites `store` anyway.
    // Theory confirmed the hard way: about 1 run in 7 did make another
    // test of this file fail this way before this isolation was
    // added here.
    let _isolation = isolate_recipe_store();
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let book = book_named(state.tree(), "Book1");

    state.open_import(book);
    let mut store = RecipeStore::default();
    store.upsert(ImportRecipe::new("Jasco"));
    let mut fluo = ImportRecipe::new("Fluo");
    fluo.separator = Separator::Tab;
    store.upsert(fluo);
    state.import_state_mut().expect("window open").store = store;

    state.select_import_recipe("Fluo");
    assert_eq!(
        state.import_state().unwrap().recipe.name,
        "Fluo",
        "the chosen template replaces the current recipe"
    );
    assert_eq!(
        state.import_state().unwrap().recipe.separator,
        Separator::Tab
    );

    state.select_import_recipe("Does not exist");
    assert_eq!(
        state.import_state().unwrap().recipe.name,
        "Fluo",
        "an unknown name replaces nothing"
    );
}

/// Saving a template without a name (or a blank name) is refused, without
/// modifying the store — condition explicitly documented by
/// `AppState::save_import_recipe`.
#[test]
fn save_import_recipe_requires_a_non_empty_name() {
    let _isolation = isolate_recipe_store();
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let book = book_named(state.tree(), "Book1");

    state.open_import(book);
    state.import_state_mut().unwrap().save_as_name = "   ".to_string();
    state.save_import_recipe();

    // S-M7 (#228): the error of a wizard gesture goes in its footer.
    assert!(
        state.dialog_error().is_some(),
        "a blank name must be refused"
    );
    // Nothing was added or written.
    assert!(
        stokesforge_import::RecipeStore::load().recipes().is_empty(),
        "nothing was added to the store"
    );
}

/// A saved template survives closing the window: reopening it
/// (new window session, but same disk) finds it as is —
/// the full round trip through `RecipeStore::save`/`load`, never
/// exercised before this test existed (the tests of `stokesforge-import` only
/// covered deserialization, never the real write).
#[test]
fn save_import_recipe_persists_to_disk_and_is_visible_after_reopening() {
    let _isolation = isolate_recipe_store();
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let book = book_named(state.tree(), "Book1");

    state.open_import(book);
    {
        let import = state.import_state_mut().unwrap();
        import.recipe.separator = Separator::Semicolon;
        import.recipe.decimal = DecimalStyle::Comma;
        import.recipe.footer_skip_lines = 2;
        import.recipe.has_column_header_row = true;
        import.recipe.name_source = NameSource::HeaderKey("TITLE".to_string());
        import.save_as_name = "Jasco UV".to_string();
    }
    state.save_import_recipe();
    let status = state.status().expect("a status message");
    assert!(!status.is_error(), "the save succeeded: {}", status.text);

    state.cancel_import();
    state.open_import(book);
    let import = state.import_state().expect("import window open");
    assert_eq!(import.recipe.name, "Jasco UV");
    assert_eq!(import.recipe.separator, Separator::Semicolon);
    assert_eq!(import.recipe.decimal, DecimalStyle::Comma);
    assert_eq!(import.recipe.footer_skip_lines, 2);
    assert!(import.recipe.has_column_header_row);
    assert_eq!(
        import.recipe.name_source,
        NameSource::HeaderKey("TITLE".to_string())
    );
}

/// Deleting a template removes it from disk, not only from the in-memory
/// list of the current window.
#[test]
fn delete_import_recipe_removes_it_from_the_store_on_disk() {
    let _isolation = isolate_recipe_store();
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let book = book_named(state.tree(), "Book1");

    let mut store = RecipeStore::default();
    store.upsert(ImportRecipe::new("Jasco"));
    store.upsert(ImportRecipe::new("Fluo"));
    store
        .save()
        .expect("writes the two isolated templates to disk");

    state.open_import(book);
    state.delete_import_recipe("Jasco");
    let status = state.status().expect("a status message");
    assert!(
        !status.is_error(),
        "the deletion succeeded: {}",
        status.text
    );

    state.cancel_import();
    state.open_import(book);
    // The saved templates.
    let user_recipes = stokesforge_import::RecipeStore::load();
    let names: Vec<&str> = user_recipes
        .recipes()
        .iter()
        .map(|r| r.name.as_str())
        .collect();
    assert_eq!(
        names,
        vec!["Fluo"],
        "\"Jasco\" did disappear from the file on disk, \"Fluo\" stays"
    );
}

/// The preview follows the current settings frame by frame (no
/// caching): without "the first row gives the column names", the header
/// row is read as unreadable data (missing values); enabling
/// it names the columns and the first real data row appears.
/// Also covers the two cases where the preview carries no table: no
/// file chosen, and a path that does not exist.
#[test]
fn import_preview_reflects_current_recipe_settings_and_reports_parse_errors() {
    // `open_import` calls `RecipeStore::load()` for real — isolated for
    // the same reason as `select_import_recipe_loads_a_saved_model_and_
    // ignores_unknown_names`, above.
    let _isolation = isolate_recipe_store();
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let book = book_named(state.tree(), "Book1");
    let file = dir.path().join("measure.csv");
    std::fs::write(&file, "Time,Signal\n1,10\n2,20\n").expect("wrote measure.csv");

    state.open_import(book);
    assert!(
        state.import_preview().is_none(),
        "no file chosen: no preview"
    );

    state.set_import_paths(vec![file.clone()]);
    let table = state
        .import_preview()
        .expect("a preview, a file is chosen")
        .expect("the file parses");
    assert!(
        table.columns[0].values[0].is_nan(),
        "without has_column_header_row, \"Time\" is read as unreadable data"
    );

    state
        .import_state_mut()
        .unwrap()
        .recipe
        .has_column_header_row = true;
    let table = state.import_preview().unwrap().unwrap();
    assert_eq!(table.columns[0].name.as_deref(), Some("Time"));
    assert_eq!(table.columns[1].name.as_deref(), Some("Signal"));
    assert_eq!(table.columns[0].values[0], 1.0);

    state.import_state_mut().unwrap().paths = vec![dir.path().join("does_not_exist.csv")];
    assert!(
        state.import_preview().unwrap().is_err(),
        "a path that cannot be found is reported, not a panic"
    );
}

/// `NameSource::HeaderKey` finds the series name in the skipped header
/// lines ("KEY<separator>VALUE" pattern), for a column
/// that otherwise has no name of its own (no column
/// header row) — the central use case of the feature, a
/// Jasco-like file with a single series.
#[test]
fn importing_resolves_series_name_from_a_header_key_when_the_column_has_no_name_of_its_own() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let book = book_named(state.tree(), "Book1");
    let file = dir.path().join("spectrum.csv");
    std::fs::write(&file, "TITLE;MySample\nXYDATA\n1;10\n2;20\n").expect("wrote spectrum.csv");

    let mut recipe = ImportRecipe::new("Untitled");
    recipe.separator = Separator::Semicolon;
    recipe.header = HeaderSkip::UntilMarker("XYDATA".to_string());
    recipe.name_source = NameSource::HeaderKey("TITLE".to_string());
    state.dispatch(Action::ImportFiles {
        destination: ImportDestination::ActiveBook(book),
        recipe,
        paths: vec![file],
    });

    let tree = state.tree();
    let sheet = sheet_named(tree, "spectrum");
    let columns = state.visible_columns(sheet);
    assert_eq!(state.column_meta(columns[0].0).long_name, "X");
    assert_eq!(
        state.column_meta(columns[1].0).long_name,
        "MySample",
        "the second column takes the name read on the TITLE line, not the file name"
    );
}

/// `has_column_header_row` names **each** column after the remaining
/// header row, not only the first — the case of a "matrix"
/// file with several Y series, each under its own name.
#[test]
fn importing_a_file_with_a_column_header_row_names_every_series_from_it() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let book = book_named(state.tree(), "Book1");
    let file = dir.path().join("matrix.txt");
    std::fs::write(&file, "wavelength\tA\tB\n1\t10\t100\n2\t20\t200\n").expect("wrote matrix.txt");

    let mut recipe = ImportRecipe::new("Untitled");
    recipe.separator = Separator::Tab;
    recipe.has_column_header_row = true;
    state.dispatch(Action::ImportFiles {
        destination: ImportDestination::ActiveBook(book),
        recipe,
        paths: vec![file],
    });

    let tree = state.tree();
    let sheet = sheet_named(tree, "matrix");
    let columns = state.visible_columns(sheet);
    assert_eq!(columns.len(), 3);
    assert_eq!(state.column_meta(columns[0].0).long_name, "wavelength");
    assert_eq!(state.column_meta(columns[1].0).long_name, "A");
    assert_eq!(state.column_meta(columns[2].0).long_name, "B");
    assert_eq!(cell_f64(&state, columns[1].0, 1), 20.0);
    assert_eq!(cell_f64(&state, columns[2].0, 0), 100.0);
}

/// Importing a single file also counts for only one undo
/// entry — the single-element batch is not a special case.
#[test]
fn undoing_a_single_file_import_removes_exactly_that_sheet() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let book = book_named(state.tree(), "Book1");
    let sheets_before = state.worksheets_of(book).len();
    let file = dir.path().join("single.csv");
    std::fs::write(&file, "1,10\n2,20\n").expect("wrote single.csv");

    state.dispatch(Action::ImportFiles {
        destination: ImportDestination::ActiveBook(book),
        recipe: ImportRecipe::new("Untitled"),
        paths: vec![file],
    });
    assert_eq!(state.worksheets_of(book).len(), sheets_before + 1);

    state.dispatch(Action::Undo);
    assert_eq!(state.worksheets_of(book).len(), sheets_before);
    assert!(!state.can_undo());
}

/// A file that reads but does not parse (header marker never
/// found) does not interrupt the batch either, and the final message reports a
/// failure — not a mute count, see the doc of `AppState::
/// import_files` (§ fix): `self.fail`/`self.notice` replace
/// `self.status`, a `self.fail` per failed file would otherwise have been
/// overwritten by the final summary message.
#[test]
fn a_parse_failure_amid_a_batch_is_surfaced_as_an_error_not_silently_swallowed() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let book = book_named(state.tree(), "Book1");
    let sheets_before = state.worksheets_of(book).len();

    let bad = dir.path().join("incomplete.csv");
    std::fs::write(&bad, "TITLE;Incomplete\n").expect("wrote incomplete.csv (never XYDATA)");
    let good = dir.path().join("complete.csv");
    std::fs::write(&good, "TITLE;Complete\nXYDATA\n1;10\n").expect("wrote complete.csv");

    let mut recipe = ImportRecipe::new("Untitled");
    recipe.separator = Separator::Semicolon;
    recipe.header = HeaderSkip::UntilMarker("XYDATA".to_string());
    state.dispatch(Action::ImportFiles {
        destination: ImportDestination::ActiveBook(book),
        recipe,
        paths: vec![bad.clone(), good],
    });

    assert_eq!(
        state.worksheets_of(book).len(),
        sheets_before + 1,
        "only the file that parses produced a sheet"
    );
    let status = state.status().expect("a status message");
    assert!(
        status.is_error(),
        "a failed file must be visible, not vanish behind a count: {}",
        status.text
    );
    assert!(
        status.text.contains("incomplete.csv"),
        "the message names the faulty file: {}",
        status.text
    );
}

/// Importing into a workbook that does not exist (invalid identifier) fails
/// cleanly — reported, never a panic, no phantom sheet.
#[test]
fn importing_into_a_nonexistent_book_fails_gracefully() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let file = dir.path().join("measure.csv");
    std::fs::write(&file, "1,10\n").expect("wrote measure.csv");

    state.dispatch(Action::ImportFiles {
        destination: ImportDestination::ActiveBook(NodeId::from_raw(999_999)),
        recipe: ImportRecipe::new("Untitled"),
        paths: vec![file],
    });

    let status = state.status().expect("a status message");
    assert!(status.is_error(), "a nonexistent workbook must be reported");
    assert!(!state.can_undo(), "nothing was created, nothing to undo");
}

/// Reads a raw numeric cell, for assertions on the imported
/// content (beyond what `cell_text` lets you see — see [`cell`]).
fn cell_f64(state: &AppState, column: NodeId, row: usize) -> f64 {
    match cell(state, column, row) {
        Cell::Float(value) => value,
        Cell::Text(_) => panic!("{column} is not a numeric column"),
    }
}

/// A cell that does not parse as a number, in the middle of an otherwise
/// numeric column, no longer becomes a mute `NaN`: the imported column
/// becomes `Mixed` and keeps the original token — this is the motivating
/// use case of the "import preview" work package, which finally consumes on the import
/// side the "mixed-type columns" work package
/// (`ParsedColumn::texts`, see `docs/import-preview-work-package.md`).
/// A fully numeric column, for its part, still produces a
/// `Numeric` column identical to before that work package — checked
/// elsewhere (all the other assertions of this file on numeric
/// imports did not change result).
#[test]
fn importing_a_non_numeric_cell_produces_a_mixed_column_with_the_original_text() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let book = book_named(state.tree(), "Book1");
    let file = dir.path().join("measure.csv");
    std::fs::write(&file, "1,2\nn/a,4\n5,6\n").expect("wrote measure.csv");
    // The text is in the first column (the X): the automatic cut-off
    // (issue #4) would stop there, hence `DataEnd::None` (no cut-off).
    let mut recipe = ImportRecipe::new("Untitled");
    recipe.data_end = stokesforge_import::DataEnd::None;

    state.dispatch(Action::ImportFiles {
        destination: ImportDestination::ActiveBook(book),
        recipe,
        paths: vec![file],
    });

    let tree = state.tree();
    let sheet = sheet_named(tree, "measure");
    let columns = state.visible_columns(sheet);
    let first = columns[0].0;
    assert_eq!(column_type(&state, first), ColumnType::Mixed);
    assert_eq!(cell_f64(&state, first, 0), 1.0);
    match cell(&state, first, 1) {
        Cell::Text(Some(text)) => assert_eq!(text, "n/a"),
        other => panic!("cell 1 should be a kept text, found {other:?}"),
    }
    assert_eq!(cell_f64(&state, first, 2), 5.0);

    let second = columns[1].0;
    assert_eq!(
        column_type(&state, second),
        ColumnType::Numeric,
        "a fully numeric column is not promoted to Mixed"
    );
}

/// The subheader roles (`long_name`/`unit`/`comment`) assigned in the
/// recipe populate `ColumnMeta` — the central feature of the
/// "import preview" work package on the model side, beyond the preview itself
/// (tested in `stokesforge-import`, see `preview::tests`).
#[test]
fn subheader_roles_populate_column_meta_long_name_unit_and_comment() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let book = book_named(state.tree(), "Book1");
    let file = dir.path().join("instrument.csv");
    std::fs::write(
        &file,
        "Temperature,Pressure\ndegC,bar\nslow measurement,fast measurement\n0.5,12\n1,2\n3,4\n",
    )
    .expect("wrote instrument.csv");

    let mut recipe = ImportRecipe::new("Untitled");
    recipe.header_layout = stokesforge_import::HeaderLayout {
        main_header_lines: 0,
        subheader_lines: 4,
        auto_detect: false,
    };
    recipe.subheader_roles = stokesforge_import::SubheaderRoles {
        long_name: Some(stokesforge_import::SubheaderLineRange::single(1)),
        unit: Some(stokesforge_import::SubheaderLineRange::single(2)),
        comment: Some(stokesforge_import::SubheaderLineRange::single(3)),
        x_prime: Some(stokesforge_import::SubheaderLineRange::single(4)),
        same_line_name_unit: false,
    };
    state.dispatch(Action::ImportFiles {
        destination: ImportDestination::ActiveBook(book),
        recipe,
        paths: vec![file],
    });

    let tree = state.tree();
    let sheet = sheet_named(tree, "instrument");
    let columns = state.visible_columns(sheet);
    let meta0 = state.column_meta(columns[0].0);
    assert_eq!(meta0.long_name, "Temperature");
    assert_eq!(meta0.unit, "degC");
    assert_eq!(meta0.comment, "slow measurement");
    assert_eq!(meta0.x_prime, Some(0.5), "x' row (#164)");
    let meta1 = state.column_meta(columns[1].0);
    assert_eq!(meta1.long_name, "Pressure");
    assert_eq!(meta1.unit, "bar");
    assert_eq!(meta1.comment, "fast measurement");
    assert_eq!(meta1.x_prime, Some(12.0));
}

// -- 14. Enter on the last row: row addition bounded to the sheet --

/// Reproduces the reported bug: in `demo_project`, `Note` (text column,
/// 3 stored values) is shorter than `Time`/`Signal` (5 values) on
/// the same sheet — a case the model explicitly allows (see the
/// doc of [`demo_project`]). Committing row 2 of `Note` with Enter
/// (`Some("")`, the last *stored* value of the column, but not the
/// last *displayed* row of the sheet) must not add a row:
/// before the fix, `commit_cell_and_advance` compared `row + 1` to the
/// buffer length of the edited column alone rather than to the real
/// height of the sheet, and added a phantom row at the bottom of the whole
/// sheet.
#[test]
fn committing_the_last_stored_value_of_a_short_column_mid_sheet_does_not_add_a_row() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let note = column_id(&state, sheet1, "Note");
    assert_eq!(state.row_count(sheet1), 5, "Time/Signal make 5 rows");

    state.dispatch(Action::CommitCellAndAdvance {
        column: note,
        row: 2,
        text: "middle".to_string(),
        sheet: sheet1,
    });

    assert_eq!(
        state.row_count(sheet1),
        5,
        "editing a row in the middle of the sheet adds no row, \
         even if it was the last stored value of this column"
    );
    assert_eq!(state.cell_text(note, 2), "middle");
}

/// Conversely, committing the real last row of the sheet with Enter
/// adds a row indeed — intended behavior, unchanged by the fix.
#[test]
fn committing_the_last_row_of_the_sheet_adds_a_row() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let time_col = column_id(&state, sheet1, "Time");
    assert_eq!(state.row_count(sheet1), 5);

    state.dispatch(Action::CommitCellAndAdvance {
        column: time_col,
        row: 4,
        text: "2.5".to_string(),
        sheet: sheet1,
    });

    assert_eq!(state.row_count(sheet1), 6, "last row: a row is added");
    assert_eq!(cell(&state, time_col, 4), Cell::Float(2.5));
}

// -- 15. Column type conversion --

/// Manual choice of a column's type (`Action::SetColumnType`): a
/// text column containing non-numeric text loses that text when becoming
/// numeric (`NaN`), a destructive conversion that is assumed — same policy
/// as `Column::set_column_type` on the `stokesforge-core` side.
#[test]
fn set_column_type_action_converts_and_can_lose_non_numeric_text() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let note = column_id(&state, sheet1, "Note");
    assert_eq!(column_type(&state, note), ColumnType::Mixed);

    state.dispatch(Action::SetColumnType {
        column: note,
        column_type: ColumnType::Numeric,
    });

    assert_eq!(column_type(&state, note), ColumnType::Numeric);
    match cell(&state, note, 0) {
        Cell::Float(value) => assert!(value.is_nan(), "\"a\" is not a number: it disappears"),
        other => panic!("numeric cell expected, got {other:?}"),
    }
}

/// A text column whose present values all parse as
/// numbers converts itself to numeric as soon as a cell is committed
/// — counterpart of the reverse direction already covered by
/// `non_numeric_input_promotes_the_column_to_mixed` (a numeric column that
/// receives text).
#[test]
fn a_text_column_left_with_only_numeric_values_auto_promotes_to_numeric() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let note = column_id(&state, sheet1, "Note");
    assert_eq!(column_type(&state, note), ColumnType::Mixed);

    // `Note` = [Some("a"), None, Some("")]: replacing "a" with a number
    // leaves row 2 (`Some("")`, present but non-numeric) in the
    // way — the column does not switch as long as a *present* value
    // stays non-numeric (`None`, for its part, is ignored: it is an
    // absent value, not a text that refuses to parse).
    state.dispatch(Action::CommitCell {
        column: note,
        row: 0,
        text: "1".to_string(),
    });
    assert_eq!(
        column_type(&state, note),
        ColumnType::Mixed,
        "row 2 (present empty string) is not a number: no conversion"
    );

    state.dispatch(Action::CommitCell {
        column: note,
        row: 2,
        text: "2.5".to_string(),
    });

    assert_eq!(
        column_type(&state, note),
        ColumnType::Numeric,
        "all present values are numbers: automatic conversion"
    );
    assert_eq!(cell(&state, note, 0), Cell::Float(1.0));
    assert_eq!(cell(&state, note, 2), Cell::Float(2.5));
}

/// `Action::SetColumnRole` (the "Role" submenu of the header context menu,
/// next to the type) writes `ColumnMeta::role` — default role `Y` for any
/// column, see `Column::new` on the `stokesforge-core` side.
#[test]
fn set_column_role_action_writes_the_column_meta_role() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let time_col = column_id(&state, sheet1, "Time");
    assert_eq!(state.column_meta(time_col).role, ColumnRole::Y);

    state.dispatch(Action::SetColumnRole {
        column: time_col,
        role: ColumnRole::X,
    });

    assert_eq!(state.column_meta(time_col).role, ColumnRole::X);
}

// -- 16. Multi-selection of columns and header fields --

/// Drag/Shift-click on the letters row (`extend_column_selection`)
/// selects a range of whole columns, not just one.
#[test]
fn extending_a_column_selection_covers_the_whole_range() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let time_col = column_id(&state, sheet1, "Time");
    let signal = column_id(&state, sheet1, "Signal");
    let note = column_id(&state, sheet1, "Note");

    state.select_column(sheet1, time_col);
    state.extend_column_selection(sheet1, note);

    assert!(state.is_column_selected(sheet1, time_col));
    assert!(
        state.is_column_selected(sheet1, signal),
        "intermediate column included"
    );
    assert!(state.is_column_selected(sheet1, note));
}

/// Drag/Shift-click on the header field rows
/// (`extend_header_field_selection`) selects a rectangle of fields across
/// several columns and several fields, not just one.
#[test]
fn extending_a_header_field_selection_covers_the_whole_rectangle() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let time_col = column_id(&state, sheet1, "Time");
    let signal = column_id(&state, sheet1, "Signal");

    state.select_header_field(sheet1, time_col, HeaderField::Unit);
    state.extend_header_field_selection(sheet1, signal, HeaderField::Formula);

    for field in [
        HeaderField::Unit,
        HeaderField::Comment,
        HeaderField::Formula,
    ] {
        assert!(
            state.is_header_field_selected(sheet1, time_col, field),
            "{field:?} of Time in the rectangle"
        );
        assert!(
            state.is_header_field_selected(sheet1, signal, field),
            "{field:?} of Signal in the rectangle"
        );
    }
    assert!(
        !state.is_header_field_selected(sheet1, time_col, HeaderField::LongName),
        "Long Name is above the anchor (Unit): outside the rectangle"
    );
    assert_eq!(
        state.selected_header_field(sheet1),
        None,
        "a rectangle of several fields is never a single field"
    );
}

/// Dragging from a header field to a data cell
/// (`extend_selection`, the *data cell* method, called while
/// the current selection is already a range of fields) stops at the
/// boundary rather than crossing it: the rectangle covers the fields
/// between the anchor and Formula (the last header field), on the target
/// column as on the anchor's, but never touches a data row.
/// Three successive fixes had kept alive a mixed rectangle that
/// crossed this boundary; see the head doc of `crate::selection`
/// for why it was removed.
#[test]
fn dragging_a_cell_extension_across_the_header_boundary_stops_at_the_formula_field() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let time_col = column_id(&state, sheet1, "Time");
    let signal = column_id(&state, sheet1, "Signal");

    state.select_header_field(sheet1, time_col, HeaderField::Unit);
    state.extend_selection(sheet1, signal, 2);

    assert!(
        !state.is_selected(sheet1, signal, 2),
        "the target cell is never included anymore: the drag stops at the boundary"
    );
    assert!(
        state.is_header_field_selected(sheet1, time_col, HeaderField::Unit),
        "the anchor field stays selected"
    );
    assert!(
        state.is_header_field_selected(sheet1, time_col, HeaderField::Comment),
        "intermediate field between the anchor and Formula included"
    );
    assert!(
        state.is_header_field_selected(sheet1, signal, HeaderField::Formula),
        "the rectangle extends to Formula on the target column"
    );
    assert!(
        !state.is_selected(sheet1, time_col, 0),
        "no data of the anchor column is included"
    );
    assert_eq!(
        state.selected_header_field(sheet1),
        None,
        "a multi-cell rectangle is never reduced to a single field"
    );
}

/// Symmetric: dragging from a data cell to a header field
/// stops at row 0, never climbs into the header.
#[test]
fn dragging_a_header_field_extension_across_the_data_boundary_stops_at_row_zero() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let time_col = column_id(&state, sheet1, "Time");

    state.select_cell(sheet1, time_col, 3);
    state.extend_header_field_selection(sheet1, time_col, HeaderField::Comment);

    assert!(
        !state.is_header_field_selected(sheet1, time_col, HeaderField::Comment),
        "the target field is never included: the drag stops at the boundary"
    );
    for row in 0..=3 {
        assert!(
            state.is_selected(sheet1, time_col, row),
            "row {row} included"
        );
    }
}

/// A selection started on the Comment field and extended to a
/// data row of the same column stops at Formula (the last
/// header field): it never touches a single data row.
/// Regression of the historical bug (three successive fixes) of the
/// "mixed rectangle", see the head doc of `crate::selection`.
#[test]
fn extending_from_a_header_field_into_data_stops_before_any_data_row() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let time_col = column_id(&state, sheet1, "Time");

    state.select_header_field(sheet1, time_col, HeaderField::Comment);
    state.extend_selection(sheet1, time_col, 4);

    assert!(state.is_header_field_selected(sheet1, time_col, HeaderField::Comment));
    assert!(
        state.is_header_field_selected(sheet1, time_col, HeaderField::Formula),
        "Formula, last header field, reached"
    );
    assert!(
        !state.is_header_field_selected(sheet1, time_col, HeaderField::Unit),
        "Unit is above the anchor: outside the rectangle"
    );
    for row in 0..=4 {
        assert!(
            !state.is_selected(sheet1, time_col, row),
            "row {row} never included: the drag stopped at Formula"
        );
    }

    let text = state.copy_selection(sheet1).expect("non-empty selection");
    assert_eq!(
        text.lines().count(),
        2,
        "2 header fields (Comment, Formula), no data row"
    );
}

/// Dragging from a data cell to the letters row
/// (`extend_column_selection` called while the current selection is a
/// rectangle of cells) sets a single-column selection at the
/// target rather than being ignored.
#[test]
fn dragging_a_column_extension_across_the_data_boundary_reselects_a_single_column() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let time_col = column_id(&state, sheet1, "Time");
    let signal = column_id(&state, sheet1, "Signal");

    state.select_cell(sheet1, time_col, 0);
    state.extend_column_selection(sheet1, signal);

    assert!(
        state.is_column_selected(sheet1, signal),
        "the target column becomes the new selection"
    );
    assert!(
        !state.is_selected(sheet1, time_col, 0),
        "the old cell selection no longer applies"
    );
}

/// Dragging from the letters row to a header field row
/// (`extend_header_field_selection` called while the current selection
/// is a range of columns) sets a single-field selection at the
/// target rather than being ignored.
#[test]
fn dragging_a_header_field_extension_across_the_column_boundary_reselects_a_single_field() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let time_col = column_id(&state, sheet1, "Time");
    let signal = column_id(&state, sheet1, "Signal");

    state.select_column(sheet1, time_col);
    state.extend_header_field_selection(sheet1, signal, HeaderField::Comment);

    assert_eq!(
        state.selected_header_field(sheet1),
        Some((signal, HeaderField::Comment)),
        "the target field becomes the new selection"
    );
    assert!(
        !state.is_column_selected(sheet1, time_col),
        "the old range of columns no longer applies"
    );
}

/// Up arrow from the first data row reaches the Formula field
/// (the lowest of the 4 header fields); Down from Formula goes back to the
/// first data row — the boundary between the header and the data
/// is now crossed with the keyboard.
#[test]
fn arrow_up_from_the_first_data_row_enters_the_header_fields() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let time_col = column_id(&state, sheet1, "Time");

    state.select_cell(sheet1, time_col, 0);
    state.move_selection(sheet1, MoveDirection::Up, false);
    assert_eq!(
        state.selected_header_field(sheet1),
        Some((time_col, HeaderField::Formula))
    );

    state.move_selection(sheet1, MoveDirection::Up, false);
    assert_eq!(
        state.selected_header_field(sheet1),
        Some((time_col, HeaderField::Comment))
    );

    state.move_selection(sheet1, MoveDirection::Down, false);
    assert_eq!(
        state.selected_header_field(sheet1),
        Some((time_col, HeaderField::Formula))
    );

    state.move_selection(sheet1, MoveDirection::Down, false);
    assert!(
        state.is_selected(sheet1, time_col, 0),
        "Down from Formula goes back down to the first data row"
    );
}

/// Tab from the Formula field of the **last** visible column enters
/// the data, at `Data(0)` of the **first** column — the counterpart of
/// Tab for the header/data boundary, symmetric to Shift+Tab from
/// `Data(0)` of the first column (see the next test).
#[test]
fn tab_from_the_last_column_formula_field_enters_the_data() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let time_col = column_id(&state, sheet1, "Time");
    let note = column_id(&state, sheet1, "Note");

    state.select_header_field(sheet1, note, HeaderField::Formula);
    state.move_selection_tab(sheet1, false);

    assert!(
        state.is_selected(sheet1, time_col, 0),
        "Tab from Formula of the last column reaches Data(0) of the first"
    );
}

/// Shift+Tab from `Data(0)` of the first visible column reaches the
/// Formula field of the **last** column — the symmetric entry point in
/// the header.
#[test]
fn shift_tab_from_the_first_cell_reaches_the_last_column_formula_field() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let time_col = column_id(&state, sheet1, "Time");
    let note = column_id(&state, sheet1, "Note");

    state.select_cell(sheet1, time_col, 0);
    state.move_selection_tab(sheet1, true);

    assert_eq!(
        state.selected_header_field(sheet1),
        Some((note, HeaderField::Formula)),
    );
}

/// Tab moves normally within the header, column by column, then
/// field by field once all columns have been traversed for a given
/// field — without ever entering the data until it is the
/// last column at the last field.
#[test]
fn tab_cycles_through_header_fields_before_reaching_data() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let time_col = column_id(&state, sheet1, "Time");
    let signal = column_id(&state, sheet1, "Signal");

    state.select_header_field(sheet1, time_col, HeaderField::LongName);
    state.move_selection_tab(sheet1, false);
    assert_eq!(
        state.selected_header_field(sheet1),
        Some((signal, HeaderField::LongName)),
        "Tab first advances column by column for the same field"
    );
}

// -- 17. Selecting whole rows (row number gutter) --

/// A click on the gutter (`select_row`) selects the whole row:
/// all the visible columns of the sheet, at that row — mirror of
/// [`extending_a_column_selection_covers_the_whole_range`] but for a
/// single row.
#[test]
fn clicking_the_row_gutter_selects_the_whole_row() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let time_col = column_id(&state, sheet1, "Time");
    let signal = column_id(&state, sheet1, "Signal");
    let note = column_id(&state, sheet1, "Note");

    state.select_row(sheet1, 1);

    assert!(state.is_row_selected(sheet1, 1));
    assert!(!state.is_row_selected(sheet1, 0), "row 0 not touched");
    assert!(state.is_selected(sheet1, time_col, 1));
    assert!(state.is_selected(sheet1, signal, 1));
    assert!(
        state.is_selected(sheet1, note, 1),
        "Note is shorter than the two other columns, but is still \
         touched by the row selection: a whole row covers \
         all the visible columns, not only the long enough ones"
    );
    assert!(
        !state.is_selected(sheet1, time_col, 0),
        "only row 1 is selected"
    );
}

/// Shift-click or drag on the gutter (`extend_row_selection`) extends the
/// range to several rows — mirror of
/// [`extending_a_column_selection_covers_the_whole_range`].
#[test]
fn extending_a_row_selection_covers_the_whole_range() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let time_col = column_id(&state, sheet1, "Time");

    state.select_row(sheet1, 1);
    state.extend_row_selection(sheet1, 3);

    for row in 1..=3 {
        assert!(state.is_row_selected(sheet1, row), "row {row} included");
        assert!(state.is_selected(sheet1, time_col, row));
    }
    assert!(!state.is_row_selected(sheet1, 0));
    assert!(!state.is_row_selected(sheet1, 4));

    // The drag direction does not matter: dragging back up covers the
    // same range.
    state.select_row(sheet1, 3);
    state.extend_row_selection(sheet1, 1);
    for row in 1..=3 {
        assert!(state.is_row_selected(sheet1, row));
    }
}

/// Backspace (`Action::ClearSelectedCells`) on a range of rows
/// clears all the columns touched at that row, without shifting anything —
/// same rule as for a rectangle of cells.
#[test]
fn backspace_on_a_full_row_selection_clears_it_without_shifting() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let time_col = column_id(&state, sheet1, "Time");

    // "Time" = [0.0, 0.5, 1.0, 1.5, 2.0].
    state.select_row(sheet1, 1);
    state.dispatch(Action::ClearSelectedCells { sheet: sheet1 });

    assert!(
        matches!(cell(&state, time_col, 1), Cell::Float(v) if v.is_nan()),
        "row 1 is empty"
    );
    assert_eq!(
        cell(&state, time_col, 2),
        Cell::Float(1.0),
        "nothing was shifted"
    );
    assert_eq!(state.row_count(sheet1), 5, "no row removed");
}

/// Deleting a selection of whole columns removes the columns from the
/// sheet (and not only their cells); Undo gives them back at once,
/// with their values. Regression: no interface path called
/// `Command::DeleteColumn`, "Delete and Shift Up" only cleared the cells.
#[test]
fn deleting_selected_columns_removes_them_and_undo_restores_them() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let time_col = column_id(&state, sheet1, "Time");
    let signal = column_id(&state, sheet1, "Signal");
    let before = state.visible_columns(sheet1).len();

    state.select_column(sheet1, time_col);
    state.extend_column_selection(sheet1, signal);
    state.dispatch(Action::DeleteSelectedCells { sheet: sheet1 });

    let after: Vec<NodeId> = state
        .visible_columns(sheet1)
        .into_iter()
        .map(|(id, ..)| id)
        .collect();
    assert_eq!(after.len(), before - 2, "the two columns are gone");
    assert!(!after.contains(&time_col) && !after.contains(&signal));

    state.dispatch(Action::Undo);
    assert_eq!(
        state.visible_columns(sheet1).len(),
        before,
        "a single Undo gives back both columns"
    );
    assert_eq!(cell(&state, time_col, 1), Cell::Float(0.5));
}

/// Delete (`Action::DeleteSelectedCells`) on a range of rows deletes the
/// row and shifts what follows up, in **all** the touched
/// columns, regardless of their own length.
#[test]
fn delete_on_a_full_row_selection_shifts_every_column_up() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let time_col = column_id(&state, sheet1, "Time");
    let signal = column_id(&state, sheet1, "Signal");
    let note = column_id(&state, sheet1, "Note");

    // "Time" = [0.0, 0.5, 1.0, 1.5, 2.0], "Signal" = [10, NaN, 30, 40, 50],
    // "Note" = [Some("a"), None, Some("")] (shorter than the two
    // others: row 1 is within its bounds).
    state.select_row(sheet1, 1);
    state.dispatch(Action::DeleteSelectedCells { sheet: sheet1 });

    assert_eq!(state.row_count(sheet1), 4);
    assert_eq!(
        cell(&state, time_col, 1),
        Cell::Float(1.0),
        "the original row 2 (1.0) took the place of the deleted row 1"
    );
    assert_eq!(
        cell(&state, time_col, 3),
        Cell::Float(2.0),
        "last row unchanged, just shifted"
    );
    assert!(matches!(cell(&state, signal, 1), Cell::Float(v) if (v - 30.0).abs() < f64::EPSILON));
    assert_eq!(
        state.cell_text(note, 1),
        "",
        "Note[2] (present empty string) moved up to position 1"
    );
}

/// The edge case explicitly targeted by the task: deleting a row
/// *beyond* the end of a column shorter than the others (`Note`, 3
/// values on a sheet of 5 rows) must neither panic nor touch that
/// column — `Command::DeleteCells` applies `at`/`count` independently per
/// column, a column shorter than `at` has nothing to remove.
#[test]
fn deleting_the_last_row_leaves_a_shorter_column_untouched() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let time_col = column_id(&state, sheet1, "Time");
    let signal = column_id(&state, sheet1, "Signal");
    let note = column_id(&state, sheet1, "Note");

    let note_before = [
        state.cell_text(note, 0),
        state.cell_text(note, 1),
        state.cell_text(note, 2),
    ];

    // Row 4 (the last, 0-indexed): outside the bounds of `Note` (length
    // 3), but well within those of `Time`/`Signal` (length 5).
    state.select_row(sheet1, 4);
    state.dispatch(Action::DeleteSelectedCells { sheet: sheet1 });

    assert_eq!(state.row_count(sheet1), 4, "Time/Signal lost a row");
    assert_eq!(
        cell(&state, time_col, 3),
        Cell::Float(1.5),
        "the last remaining row is the original second to last"
    );
    assert!(matches!(cell(&state, signal, 3), Cell::Float(v) if (v - 40.0).abs() < f64::EPSILON));
    assert_eq!(
        [
            state.cell_text(note, 0),
            state.cell_text(note, 1),
            state.cell_text(note, 2),
        ],
        note_before,
        "Note, shorter than the deleted row, did not move"
    );
}

/// [`AppState::grid_menu`] on a range of rows returns the `Rows` zone and
/// the right labels, singular and plural — the plural there follows only the
/// number of *rows*, never the number of columns (see the doc of
/// [`GridMenu::plural`]): a single row on a 3-column sheet
/// stays "the row".
#[test]
fn the_grid_menu_on_a_full_row_selection_names_rows() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");

    state.select_row(sheet1, 1);
    let menu = state.grid_menu(sheet1).expect("a selected row");
    assert_eq!(menu.zone, GridZone::Rows);
    assert!(
        !menu.plural,
        "a single row stays singular even if it touches 3 columns"
    );
    assert_eq!(menu.target_label(), "the row");
    assert!(
        menu.offers_delete_shift,
        "Delete shifts up what follows in all the columns"
    );

    state.extend_row_selection(sheet1, 3);
    let menu = state.grid_menu(sheet1).expect("a range of rows");
    assert!(menu.plural, "several rows covered");
    assert_eq!(menu.target_label(), "the rows");
}

/// Copying a whole row (Ctrl+C on the gutter) then pasting it
/// elsewhere pastes each column in its place, like a rectangle of cells
/// — it is the `full_rank_range`/`DataRows` matrix of [`AppState::
/// copy_selection`]/[`AppState::paste`] that applies, not the
/// "range of whole columns" branch (which makes no sense for a row).
#[test]
fn copying_a_full_row_and_pasting_elsewhere_pastes_the_whole_row() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let time_col = column_id(&state, sheet1, "Time");
    let signal = column_id(&state, sheet1, "Signal");
    let note = column_id(&state, sheet1, "Note");

    state.select_row(sheet1, 0);
    let text = state.copy_selection(sheet1).expect("non-empty row");
    assert_eq!(text, "0\t10\ta\n");

    state.select_row(sheet1, 1);
    state.dispatch(Action::Paste {
        sheet: sheet1,
        text,
    });

    assert_eq!(cell(&state, time_col, 1), Cell::Float(0.0));
    assert!(matches!(cell(&state, signal, 1), Cell::Float(v) if (v - 10.0).abs() < f64::EPSILON));
    assert_eq!(state.cell_text(note, 1), "a");
    assert_eq!(
        cell(&state, time_col, 0),
        Cell::Float(0.0),
        "the copied row itself did not move"
    );
    assert_eq!(
        cell(&state, time_col, 2),
        Cell::Float(1.0),
        "the rows that did not receive the paste did not move"
    );
}

/// Dragging from a data cell to the gutter sets a
/// single-row selection at the target rather than being ignored — counterpart
/// of [`dragging_a_column_extension_across_the_data_boundary_reselects_a_single_column`].
#[test]
fn dragging_a_row_extension_across_the_data_boundary_reselects_a_single_row() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let time_col = column_id(&state, sheet1, "Time");

    state.select_cell(sheet1, time_col, 0);
    state.extend_row_selection(sheet1, 3);

    assert!(
        state.is_row_selected(sheet1, 3),
        "the target row becomes the new selection"
    );
    assert!(
        !state.is_selected(sheet1, time_col, 0),
        "the old cell selection no longer applies"
    );
}

/// Dragging from the gutter to a data cell sets a
/// single-cell selection at the target rather than silently extending
/// the range of rows to no effect — counterpart of
/// [`dragging_a_column_extension_across_the_data_boundary_reselects_a_single_column`]
/// on the column side.
#[test]
fn dragging_a_cell_extension_across_the_row_gutter_boundary_reselects_a_single_cell() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let time_col = column_id(&state, sheet1, "Time");

    state.select_row(sheet1, 1);
    state.extend_selection(sheet1, time_col, 3);

    assert_eq!(
        state.selection(sheet1).and_then(Selection::single_cell),
        Some((time_col, 3)),
        "the target cell becomes the new selection"
    );
    assert!(
        !state.is_row_selected(sheet1, 1),
        "the old range of rows no longer applies"
    );
}

/// Dragging from the letters row to the gutter sets a
/// single-row selection, and conversely — the two whole
/// ranges never cross each other either.
#[test]
fn dragging_between_the_letter_row_and_the_row_gutter_never_mixes_the_two_ranges() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let time_col = column_id(&state, sheet1, "Time");

    state.select_column(sheet1, time_col);
    state.extend_row_selection(sheet1, 2);
    assert!(state.is_row_selected(sheet1, 2));
    assert!(!state.is_column_selected(sheet1, time_col));

    state.select_row(sheet1, 2);
    state.extend_column_selection(sheet1, time_col);
    assert!(state.is_column_selected(sheet1, time_col));
    assert!(!state.is_row_selected(sheet1, 2));
}

// -- 18. Selecting a whole header field (gutter labels) --

/// A click on a field label in the gutter (`select_header_row`)
/// selects that field for **all** the visible columns of the sheet
/// — mirror of [`clicking_the_row_gutter_selects_the_whole_row`], on the
/// header side.
#[test]
fn clicking_a_header_row_label_selects_the_field_in_every_column() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let time_col = column_id(&state, sheet1, "Time");
    let signal = column_id(&state, sheet1, "Signal");
    let note = column_id(&state, sheet1, "Note");

    set_header_field(&mut state, time_col, HeaderField::LongName, "T");
    set_header_field(&mut state, signal, HeaderField::LongName, "S");

    state.select_header_row(sheet1, HeaderField::LongName);

    assert!(state.is_header_row_selected(sheet1, HeaderField::LongName));
    assert!(!state.is_header_row_selected(sheet1, HeaderField::Unit));
    assert!(state.is_header_field_selected(sheet1, time_col, HeaderField::LongName));
    assert!(state.is_header_field_selected(sheet1, signal, HeaderField::LongName));
    assert!(
        state.is_header_field_selected(sheet1, note, HeaderField::LongName),
        "Note is a visible column like the others, even without a long name \
         defined: a whole field covers all the visible columns"
    );
    assert!(
        !state.is_header_field_selected(sheet1, time_col, HeaderField::Unit),
        "only Long Name is selected"
    );
}

/// Shift-click or drag on the labels (`extend_header_row_selection`)
/// extends the range to several fields — mirror of
/// [`extending_a_row_selection_covers_the_whole_range`].
#[test]
fn extending_a_header_row_selection_covers_the_whole_field_range() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let time_col = column_id(&state, sheet1, "Time");

    state.select_header_row(sheet1, HeaderField::Unit);
    state.extend_header_row_selection(sheet1, HeaderField::Formula);

    for field in [
        HeaderField::Unit,
        HeaderField::Comment,
        HeaderField::Formula,
    ] {
        assert!(state.is_header_row_selected(sheet1, field));
        assert!(state.is_header_field_selected(sheet1, time_col, field));
    }
    assert!(!state.is_header_row_selected(sheet1, HeaderField::LongName));

    // The drag direction does not matter.
    state.select_header_row(sheet1, HeaderField::Formula);
    state.extend_header_row_selection(sheet1, HeaderField::Unit);
    for field in [
        HeaderField::Unit,
        HeaderField::Comment,
        HeaderField::Formula,
    ] {
        assert!(state.is_header_row_selected(sheet1, field));
    }
}

/// Backspace on a range of a whole header field clears that field in
/// **all** the columns — same mechanism as
/// [`delete_on_a_header_field_selection_clears_it_like_backspace`], but on
/// all the columns in a single gesture rather than just one.
#[test]
fn clearing_a_header_row_selection_empties_the_field_in_every_column() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let time_col = column_id(&state, sheet1, "Time");
    let signal = column_id(&state, sheet1, "Signal");

    set_header_field(&mut state, time_col, HeaderField::Unit, "s");
    set_header_field(&mut state, signal, HeaderField::Unit, "V");
    set_header_field(&mut state, time_col, HeaderField::LongName, "T");

    state.select_header_row(sheet1, HeaderField::Unit);
    state.dispatch(Action::ClearSelectedCells { sheet: sheet1 });

    assert_eq!(state.column_meta(time_col).unit, "");
    assert_eq!(state.column_meta(signal).unit, "");
    assert_eq!(
        state.column_meta(time_col).long_name,
        "T",
        "only the Unit field was selected"
    );
}

/// Delete on a range of a whole header field just clears,
/// exactly as on a field of a single column: a field is not
/// repeated per row, there is nothing to shift up (see
/// [`Selection::delete_at_count`]).
#[test]
fn delete_on_a_header_row_selection_only_clears_it() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let time_col = column_id(&state, sheet1, "Time");
    let signal = column_id(&state, sheet1, "Signal");

    set_header_field(&mut state, time_col, HeaderField::Comment, "c1");
    set_header_field(&mut state, signal, HeaderField::Comment, "c2");

    state.select_header_row(sheet1, HeaderField::Comment);
    state.dispatch(Action::DeleteSelectedCells { sheet: sheet1 });

    assert_eq!(state.column_meta(time_col).comment, "");
    assert_eq!(state.column_meta(signal).comment, "");
    assert_eq!(
        cell(&state, time_col, 0),
        Cell::Float(0.0),
        "no data moved up"
    );
}

/// [`AppState::grid_menu`] on a range of a whole header field stays
/// in the `Header` zone (same terminology as a rectangle of fields of a
/// single column) and does not offer "Delete and Shift Up".
#[test]
fn the_grid_menu_on_a_header_row_selection_stays_in_the_header_zone() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");

    state.select_header_row(sheet1, HeaderField::LongName);
    let menu = state.grid_menu(sheet1).expect("a whole field selected");
    assert_eq!(menu.zone, GridZone::Header);
    assert!(
        menu.plural,
        "a whole field touches several columns on Sheet1"
    );
    assert_eq!(menu.target_label(), "the fields");
    assert!(
        !menu.offers_delete_shift,
        "a field is not repeated per row, Delete has nothing to shift"
    );
}

/// Copying a range of a whole header field (Ctrl+C on a label) then
/// pasting it elsewhere writes each column in its place, like a rectangle of
/// fields — it is the `full_rank_range`/`HeaderFields` matrix of
/// [`AppState::copy_selection`]/[`AppState::paste`] that applies.
#[test]
fn copying_a_header_row_and_pasting_elsewhere_pastes_every_column() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let time_col = column_id(&state, sheet1, "Time");
    let signal = column_id(&state, sheet1, "Signal");

    set_header_field(&mut state, time_col, HeaderField::LongName, "T");
    set_header_field(&mut state, signal, HeaderField::LongName, "S");

    state.select_header_row(sheet1, HeaderField::LongName);
    let text = state.copy_selection(sheet1).expect("non-empty field");
    assert_eq!(
        text, "T\tS\t\n",
        "Sheet1 has a third column (Note), with no long name defined"
    );

    set_header_field(&mut state, time_col, HeaderField::Unit, "existant");
    state.select_header_row(sheet1, HeaderField::Unit);
    state.dispatch(Action::Paste {
        sheet: sheet1,
        text,
    });

    assert_eq!(state.column_meta(time_col).unit, "T");
    assert_eq!(state.column_meta(signal).unit, "S");
    assert_eq!(
        state.column_meta(time_col).long_name,
        "T",
        "the copied field itself did not move"
    );
}

/// Extending a data cell selection resets an ongoing range of a
/// whole header field rather than silently extending to no
/// effect — lock for the guard added to [`AppState::extend_selection`].
#[test]
fn extending_a_data_selection_resets_a_header_rows_selection() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let time_col = column_id(&state, sheet1, "Time");

    state.select_header_row(sheet1, HeaderField::LongName);
    state.extend_selection(sheet1, time_col, 2);

    assert_eq!(
        state.selection(sheet1).and_then(Selection::single_cell),
        Some((time_col, 2)),
        "the target cell becomes the new selection"
    );
    assert!(!state.is_header_row_selected(sheet1, HeaderField::LongName));
}

/// Extending a single-column field resets an ongoing range of a
/// whole field rather than staying stuck on the old range without
/// changing it — lock for the guard added to [`AppState::
/// extend_header_field_selection`]. Without this guard, the call would have done
/// nothing at all: `extend_row_to` only mutates `SelectionKind::Rows`, never
/// `HeaderRows`, the selection would have stayed frozen on the old whole field.
#[test]
fn extending_a_single_column_header_field_resets_a_header_rows_selection() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let time_col = column_id(&state, sheet1, "Time");

    state.select_header_row(sheet1, HeaderField::Unit);
    state.extend_header_field_selection(sheet1, time_col, HeaderField::Comment);

    assert_eq!(
        state.selected_header_field(sheet1),
        Some((time_col, HeaderField::Comment)),
        "the selection restarts from a single field, the targeted one"
    );
    assert!(!state.is_header_row_selected(sheet1, HeaderField::Unit));
}

/// Dragging between the letters row and a whole-field label sets
/// a simple selection in both directions — the two "whole" ranges never
/// cross each other, like the letters row and the row number
/// gutter.
#[test]
fn dragging_between_the_letter_row_and_a_header_row_label_never_mixes_the_two_ranges() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let time_col = column_id(&state, sheet1, "Time");

    state.select_column(sheet1, time_col);
    state.extend_header_row_selection(sheet1, HeaderField::Unit);
    assert!(state.is_header_row_selected(sheet1, HeaderField::Unit));
    assert!(!state.is_column_selected(sheet1, time_col));

    state.select_header_row(sheet1, HeaderField::Unit);
    state.extend_column_selection(sheet1, time_col);
    assert!(state.is_column_selected(sheet1, time_col));
    assert!(!state.is_header_row_selected(sheet1, HeaderField::Unit));
}

// -- 19. Selecting the whole table ("#"), and "touched" gutter --

/// A click on the "#" (`select_all`) selects the whole table: all
/// the rows, hence all the visible columns — `is_all_selected` confirms
/// it, and the grid context menu does stay in the data/rows
/// zone (no header) since the whole table is part of it.
#[test]
fn clicking_the_hash_gutter_selects_the_whole_table() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let time_col = column_id(&state, sheet1, "Time");
    let signal = column_id(&state, sheet1, "Signal");
    let note = column_id(&state, sheet1, "Note");

    state.select_all(sheet1);

    assert!(state.is_all_selected(sheet1));
    for row in 0..state.row_count(sheet1) {
        assert!(state.is_selected(sheet1, time_col, row));
        assert!(state.is_selected(sheet1, signal, row));
        assert!(
            state.is_selected(sheet1, note, row),
            "Note is shorter, but a selection of the whole table \
             covers all the visible columns, not only the \
             long enough ones"
        );
    }
}

/// A sheet with no rows can still be selected entirely (#159): the
/// corner cell and Ctrl+A take all the visible columns, header
/// included, which allows for example changing their type or role.
#[test]
fn select_all_on_a_sheet_without_rows_selects_every_column() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let time_col = column_id(&state, sheet1, "Time");
    let signal = column_id(&state, sheet1, "Signal");
    state.dispatch(Action::DeleteRows {
        sheet: sheet1,
        at: 0,
        count: state.row_count(sheet1),
    });
    assert_eq!(state.row_count(sheet1), 0);

    state.select_cell(sheet1, time_col, 0);
    state.dispatch(Action::SelectAll { sheet: sheet1 });

    assert!(state.is_all_selected(sheet1));
    assert!(state.is_column_selected(sheet1, time_col));
    assert!(state.is_column_selected(sheet1, signal));
}

/// A partial selection is not "all selected".
#[test]
fn a_partial_column_selection_is_not_all_selected() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let time_col = column_id(&state, sheet1, "Time");

    state.select_column(sheet1, time_col);

    assert!(!state.is_all_selected(sheet1));
}

/// Selecting a single data cell "touches" its column and
/// row — the marker that tints the letters row and the row number
/// gutter with an intermediate intensity, without selecting them
/// all the same (`is_column_selected`/`is_row_selected` stay false: it is
/// not the letters row nor the gutter that was clicked).
#[test]
fn selecting_a_data_cell_touches_its_column_and_row_but_does_not_select_them() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let time_col = column_id(&state, sheet1, "Time");
    let signal = column_id(&state, sheet1, "Signal");

    state.select_cell(sheet1, time_col, 2);

    assert!(state.column_touched_by_selection(sheet1, time_col));
    assert!(state.row_touched_by_selection(sheet1, 2));
    assert!(!state.column_touched_by_selection(sheet1, signal));
    assert!(!state.row_touched_by_selection(sheet1, 0));
    assert!(!state.is_column_selected(sheet1, time_col));
    assert!(!state.is_row_selected(sheet1, 2));
}

/// A header field selection (single column or whole field) never
/// touches a data row: `row_touched_by_selection` must
/// stay false, the row number gutter must not react to a
/// selection that never goes down into the data.
#[test]
fn a_header_selection_never_touches_a_data_row() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let time_col = column_id(&state, sheet1, "Time");

    state.select_header_field(sheet1, time_col, HeaderField::Unit);
    assert!(!state.row_touched_by_selection(sheet1, 0));

    state.select_header_row(sheet1, HeaderField::Unit);
    assert!(!state.row_touched_by_selection(sheet1, 0));
    assert!(
        state.column_touched_by_selection(sheet1, time_col),
        "a whole header field does touch all the columns"
    );
}

// -- 20. Tab on the last column of a data row: column addition --

/// `Action::AddColumnAndSelect` adds a column at the end of the sheet then
/// selects the requested row in it — the counterpart of
/// [`committing_the_last_row_of_the_sheet_adds_a_row`] but for the
/// column axis (Tab rather than Enter).
#[test]
fn add_column_and_select_adds_a_column_and_selects_it_on_the_given_row() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let columns_before = state.visible_columns(sheet1).len();

    state.dispatch(Action::AddColumnAndSelect {
        sheet: sheet1,
        row: 2,
    });

    let columns_after = state.visible_columns(sheet1);
    assert_eq!(columns_after.len(), columns_before + 1);
    let new_column = columns_after.last().expect("added column").0;
    assert!(
        state.is_selected(sheet1, new_column, 2),
        "the requested row is selected in the new column"
    );
}

/// `Action::AddColumnAndSelectHeaderField` — the counterpart of
/// [`add_column_and_select_adds_a_column_and_selects_it_on_the_given_row`]
/// on the header side: Tab committed on a header field of the last column
/// adds a column rather than entering the data.
#[test]
fn add_column_and_select_header_field_adds_a_column_and_selects_the_field_in_it() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let columns_before = state.visible_columns(sheet1).len();

    state.dispatch(Action::AddColumnAndSelectHeaderField {
        sheet: sheet1,
        field: HeaderField::Formula,
    });

    let columns_after = state.visible_columns(sheet1);
    assert_eq!(columns_after.len(), columns_before + 1);
    let new_column = columns_after.last().expect("added column").0;
    assert_eq!(
        state.selected_header_field(sheet1),
        Some((new_column, HeaderField::Formula)),
        "the requested field is selected in the new column"
    );
}

// -- 21. Grid body alignment: numbers right, text left --

/// A populated numeric cell is numeric — `cell_is_numeric` drives
/// `app::grid::cell_widget` to align it to the right, as in any spreadsheet.
#[test]
fn a_populated_numeric_cell_is_numeric() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let time_col = column_id(&state, sheet1, "Time");

    assert!(state.cell_is_numeric(time_col, 1));
}

/// A missing numeric cell (`NaN`) is not numeric in the sense of
/// alignment: a blank has no reason to be pushed right rather than
/// left.
#[test]
fn a_missing_numeric_cell_is_not_numeric() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let time_col = column_id(&state, sheet1, "Time");
    state.dispatch(Action::CommitCell {
        column: time_col,
        row: 2,
        text: "   ".to_string(),
    });

    assert!(!state.cell_is_numeric(time_col, 2));
}

/// A text cell is never numeric.
#[test]
fn a_text_cell_is_not_numeric() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let note = column_id(&state, sheet1, "Note");

    assert!(!state.cell_is_numeric(note, 0), "Note, row 0 = \"a\"");
}

/// On a `Mixed` column, the type is read per cell, not per column:
/// two neighboring rows of the same column can align differently.
#[test]
fn cell_is_numeric_reads_per_cell_on_a_mixed_column() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let note = column_id(&state, sheet1, "Note");
    assert_eq!(column_type(&state, note), ColumnType::Mixed);

    state.dispatch(Action::CommitCell {
        column: note,
        row: 1,
        text: "3".to_string(),
    });

    assert!(
        state.cell_is_numeric(note, 1),
        "\"3\" typed into a Mixed column stays a numeric value"
    );
    assert!(
        !state.cell_is_numeric(note, 0),
        "the neighboring row, which stayed text (\"a\"), is not"
    );
}

// -- 22. Plot from a workbook (header context menu) --

/// `Action::PlotColumns` pairs each selected Y with the nearest X column to its
/// **left** in the sheet, OriginLab style: `Time` is given the `X` role,
/// `Signal` stays `Y` (default role), so the created series must plot
/// `Signal` against `Time`.
#[test]
fn plot_columns_pairs_each_y_with_the_nearest_x_to_its_left() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let time = column_id(&state, sheet1, "Time");
    let signal = column_id(&state, sheet1, "Signal");
    let root = state.tree().root();
    state.dispatch(Action::SetColumnRole {
        column: time,
        role: ColumnRole::X,
    });

    state.dispatch(Action::PlotColumns {
        sheet: sheet1,
        columns: vec![time, signal],
        plot_type: PlotType::Line,
    });

    let graph = state
        .tree()
        .node(root)
        .and_then(|node| node.children().last().copied())
        .expect("the graph was created in the workbook folder");
    let spec = state.plot_spec(graph).expect("spec of the created graph");
    assert!(
        spec.title.is_empty(),
        "#103: plotting columns adds no title"
    );
    assert_eq!(spec.panels.len(), 1);
    let series = &spec.panels[0].series;
    assert_eq!(series.len(), 1, "Time is X, not a series; Signal is one");
    assert_eq!(series[0].y, signal);
    assert_eq!(series[0].x, Some(time));
    assert_eq!(series[0].plot_type, PlotType::Line);
}

/// #360: the Y Error column to the right of the Y, even unselected, fills
/// `y_error` of the series created by `PlotColumns`.
#[test]
fn plot_columns_pairs_the_y_error_column_to_the_right_of_the_y() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let mut project = Project::new("Errors").expect("project");
    let root = project.root();
    let book = project.add_workbook(root, Some("Book1")).expect("workbook");
    let sheet = project
        .tree()
        .child_by_name(book, "Sheet1")
        .expect("read")
        .expect("sheet");
    let mut add = |name: &str| {
        project
            .add_column(sheet, None, Some(name), Column::from_f64(&[1.0, 2.0, 3.0]))
            .expect("column")
    };
    let (x, y, e) = (add("X"), add("Y"), add("E"));
    let (_session, mut state) = start(project, &dir.path().join("errors.stf"));
    for (column, role) in [
        (x, ColumnRole::X),
        (y, ColumnRole::Y),
        (e, ColumnRole::YError),
    ] {
        state.dispatch(Action::SetColumnRole { column, role });
    }
    state.dispatch(Action::PlotColumns {
        sheet,
        columns: vec![y],
        plot_type: PlotType::Line,
    });
    let graph = state
        .tree()
        .node(root)
        .and_then(|node| node.children().last().copied())
        .expect("graph");
    let spec = state.plot_spec(graph).expect("spec");
    let series = &spec.panels[0].series;
    assert_eq!(series.len(), 1);
    assert_eq!(series[0].y_error, Some(e));
    assert_eq!(series[0].x_error, None);
}

/// #167: a Y2 is paired with the nearest X2 to its left and goes on a
/// **second layer** overlaid (X axis on top, Y axis on the right); the Y stays
/// on the first. Adding columns to the graph afterwards reuses this layer.
#[test]
fn plot_columns_sends_y2_to_a_second_layer_paired_with_x2() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let mut project = Project::new("X2Y2").expect("project");
    let root = project.root();
    let book = project.add_workbook(root, Some("Book1")).expect("workbook");
    let sheet = project
        .tree()
        .child_by_name(book, "Sheet1")
        .expect("read")
        .expect("sheet");
    let mut add = |name: &str| {
        project
            .add_column(sheet, None, Some(name), Column::from_f64(&[1.0, 2.0, 3.0]))
            .expect("column")
    };
    let (x, y, x2, y2) = (add("X"), add("Y"), add("X2"), add("Y2"));
    let (_session, mut state) = start(project, &dir.path().join("projet.stf"));
    for (column, role) in [
        (x, ColumnRole::X),
        (y, ColumnRole::Y),
        (x2, ColumnRole::X2),
        (y2, ColumnRole::Y2),
    ] {
        state.dispatch(Action::SetColumnRole { column, role });
    }

    state.dispatch(Action::PlotColumns {
        sheet,
        columns: vec![x, y, x2, y2],
        plot_type: PlotType::Line,
    });

    let graph = state
        .tree()
        .node(root)
        .and_then(|node| node.children().last().copied())
        .expect("graph created");
    let spec = state.plot_spec(graph).expect("spec");
    assert_eq!(spec.panels.len(), 2, "one more layer for Y2");
    assert_eq!(spec.panels[0].overlay, None);
    assert_eq!(
        (spec.panels[0].series[0].x, spec.panels[0].series[0].y),
        (Some(x), y)
    );
    let second = &spec.panels[1];
    assert!(second.overlay.is_some_and(|o| o.x_top && o.y_right));
    assert_eq!((second.row, second.column), (0, 0), "same cell");
    assert_eq!((second.series[0].x, second.series[0].y), (Some(x2), y2));

    // A second drop of Y2 reuses the same layer, without creating another.
    state.dispatch(Action::AddSeriesToGraph {
        graph,
        panel: 0,
        columns: vec![y2],
        plot_type: None,
    });
    let after = state.plot_spec(graph).expect("spec");
    assert_eq!(after.panels.len(), 2);
    assert_eq!(after.panels[1].series.len(), 2);
    assert_eq!(after.panels[0].series.len(), 1);

    state.dispatch(Action::Undo);
    assert_eq!(
        state.plot_spec(graph).expect("spec").panels[1].series.len(),
        1
    );
}

/// #168: Y3 and Y4 are paired with X3 and X4, each on its own numbered layer;
/// a Y3 dropped later finds its own; a Y3 without Y2 does not create an empty
/// layer 2.
#[test]
fn plot_columns_sends_y3_and_y4_to_their_own_numbered_layers() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let mut project = Project::new("X3Y4").expect("project");
    let root = project.root();
    let book = project.add_workbook(root, Some("Book1")).expect("workbook");
    let sheet = project
        .tree()
        .child_by_name(book, "Sheet1")
        .expect("read")
        .expect("sheet");
    let mut add = |name: &str| {
        project
            .add_column(sheet, None, Some(name), Column::from_f64(&[1.0, 2.0, 3.0]))
            .expect("column")
    };
    let c: Vec<NodeId> = ["X", "Y", "X3", "Y3", "X4", "Y4"].map(&mut add).to_vec();
    let (_session, mut state) = start(project, &dir.path().join("projet.stf"));
    for (column, role) in c.iter().copied().zip([
        ColumnRole::X,
        ColumnRole::Y,
        ColumnRole::X3,
        ColumnRole::Y3,
        ColumnRole::X4,
        ColumnRole::Y4,
    ]) {
        state.dispatch(Action::SetColumnRole { column, role });
    }
    state.dispatch(Action::PlotColumns {
        sheet,
        columns: c.clone(),
        plot_type: PlotType::Line,
    });
    let graph = state
        .tree()
        .node(root)
        .and_then(|node| node.children().last().copied())
        .expect("graph created");
    let spec = state.plot_spec(graph).expect("spec");
    let layers: Vec<(u8, NodeId, Option<NodeId>)> = spec
        .panels
        .iter()
        .skip(1)
        .map(|p| {
            (
                p.overlay.expect("overlaid").layer,
                p.series[0].y,
                p.series[0].x,
            )
        })
        .collect();
    assert_eq!(
        layers,
        [(3, c[3], Some(c[2])), (4, c[5], Some(c[4]))],
        "no empty layer 2, X3/X4 and not X"
    );

    // A re-dropped Y3 joins layer 3, without creating another.
    state.dispatch(Action::AddSeriesToGraph {
        graph,
        panel: 0,
        columns: vec![c[3]],
        plot_type: None,
    });
    let after = state.plot_spec(graph).expect("spec");
    assert_eq!(after.panels.len(), 3);
    assert_eq!(after.panels[1].series.len(), 2);
    assert_eq!(after.panels[2].series.len(), 1);
}

/// #168: "New Layer" refuses a fourth overlaid layer; a two-axes layer takes
/// the first free role number.
#[test]
fn add_layer_numbers_two_axes_layers_and_stops_at_three_overlays() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let signal = column_id(&state, sheet1, "Signal");
    let root = state.tree().root();
    state.dispatch(Action::PlotColumns {
        sheet: sheet1,
        columns: vec![signal],
        plot_type: PlotType::Line,
    });
    let graph = state
        .tree()
        .node(root)
        .and_then(|node| node.children().last().copied())
        .expect("graph created");
    for _ in 0..4 {
        state.dispatch(Action::AddLayer {
            graph,
            panel: 0,
            x_top: true,
            y_right: true,
        });
    }
    let spec = state.plot_spec(graph).expect("spec");
    let numbers: Vec<u8> = spec
        .panels
        .iter()
        .skip(1)
        .map(|p| p.overlay.unwrap().layer)
        .collect();
    assert_eq!(numbers, [2, 3, 4], "the fourth is refused");
}

/// #167: `AddLayer` puts an overlaid layer on the base layer of the targeted
/// layer, in one undo step; a nonexistent layer changes nothing.
#[test]
fn add_layer_overlays_the_base_of_the_target_layer() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let signal = column_id(&state, sheet1, "Signal");
    let root = state.tree().root();
    state.dispatch(Action::PlotColumns {
        sheet: sheet1,
        columns: vec![signal],
        plot_type: PlotType::Line,
    });
    let graph = state
        .tree()
        .node(root)
        .and_then(|node| node.children().last().copied())
        .expect("graph created");

    state.dispatch(Action::AddLayer {
        graph,
        panel: 0,
        x_top: true,
        y_right: false,
    });
    // From the overlaid layer, the new one covers the same base layer.
    state.dispatch(Action::AddLayer {
        graph,
        panel: 1,
        x_top: false,
        y_right: true,
    });
    let spec = state.plot_spec(graph).expect("spec");
    assert_eq!(spec.panels.len(), 3);
    assert_eq!(
        spec.panels[1].overlay.map(|o| (o.x_top, o.y_right)),
        Some((true, false))
    );
    assert_eq!(
        spec.panels[2].overlay.map(|o| (o.x_top, o.y_right)),
        Some((false, true))
    );
    assert!(spec.panels.iter().all(|p| (p.row, p.column) == (0, 0)));

    state.dispatch(Action::AddLayer {
        graph,
        panel: 9,
        x_top: true,
        y_right: true,
    });
    assert_eq!(state.plot_spec(graph).expect("spec").panels.len(), 3);

    state.dispatch(Action::Undo);
    assert_eq!(state.plot_spec(graph).expect("spec").panels.len(), 2);
}

/// Without an X column to its left (no column with the `X` role in the sheet),
/// the abscissa stays implicit: Origin's behavior with no X designated.
#[test]
fn plot_columns_without_an_x_to_the_left_uses_an_implicit_abscissa() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let signal = column_id(&state, sheet1, "Signal");
    let root = state.tree().root();

    state.dispatch(Action::PlotColumns {
        sheet: sheet1,
        columns: vec![signal],
        plot_type: PlotType::Scatter,
    });

    let graph = state
        .tree()
        .node(root)
        .and_then(|node| node.children().last().copied())
        .expect("the graph was created");
    let spec = state.plot_spec(graph).expect("spec of the created graph");
    let series = &spec.panels[0].series;
    assert_eq!(series.len(), 1);
    assert_eq!(series[0].y, signal);
    assert_eq!(series[0].x, None, "no X column in the sheet");
}

/// A selection with no column of role Y creates no graph: no command
/// sent, so nothing to undo.
#[test]
fn plot_columns_without_any_y_column_is_a_no_op() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let time = column_id(&state, sheet1, "Time");
    let root = state.tree().root();
    state.dispatch(Action::SetColumnRole {
        column: time,
        role: ColumnRole::X,
    });
    let children_before = state.tree().node(root).expect("root").children().len();
    let undo_label_before = state.undo_label();

    state.dispatch(Action::PlotColumns {
        sheet: sheet1,
        columns: vec![time],
        plot_type: PlotType::Line,
    });

    let children_after = state.tree().node(root).expect("root").children().len();
    assert_eq!(
        children_before, children_after,
        "no Y column selected: no graph created"
    );
    assert_eq!(
        state.undo_label(),
        undo_label_before,
        "no command was sent: the history entry to undo does not change"
    );
    assert!(state.status().is_some_and(|status| status.is_error()));
}

/// Undoing after `Action::PlotColumns` restores the tree bit for bit: the
/// created graph disappears, like any other creation.
#[test]
fn undo_after_plot_columns_restores_the_tree_bit_for_bit() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let signal = column_id(&state, sheet1, "Signal");
    let before = std::sync::Arc::clone(state.tree());

    state.dispatch(Action::PlotColumns {
        sheet: sheet1,
        columns: vec![signal],
        plot_type: PlotType::Line,
    });
    assert!(state.can_undo());

    state.dispatch(Action::Undo);

    assert_eq!(**state.tree(), *before);
}

/// `Action::AddSeriesToGraph` adds series to an existing `PlotSpec` without
/// overwriting those already present: this is what the drag and drop of
/// work package 2 will require, tested here by driving `AppState` directly.
#[test]
fn add_series_to_graph_appends_without_overwriting_existing_series() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let signal = column_id(&state, sheet1, "Signal");
    let note = column_id(&state, sheet1, "Note");
    let root = state.tree().root();

    state.dispatch(Action::PlotColumns {
        sheet: sheet1,
        columns: vec![signal],
        plot_type: PlotType::Line,
    });
    let graph = state
        .tree()
        .node(root)
        .and_then(|node| node.children().last().copied())
        .expect("the graph was created");
    let before = state.plot_spec(graph).expect("initial spec");
    assert_eq!(before.panels[0].series.len(), 1);

    state.dispatch(Action::AddSeriesToGraph {
        graph,
        panel: 0,
        columns: vec![note],
        plot_type: Some(PlotType::Scatter),
    });

    let after = state.plot_spec(graph).expect("spec after add");
    assert_eq!(after.panels[0].series.len(), 2);
    assert_eq!(
        after.panels[0].series[0].y, signal,
        "the series already present is not overwritten"
    );
    assert_eq!(after.panels[0].series[1].y, note);
    assert_eq!(after.panels[0].series[1].plot_type, PlotType::Scatter);
}

/// #177: series added **at the same time** form a group; a series
/// added alone has none. A new multiple add opens another group
/// without touching the first.
#[test]
fn series_added_together_form_a_group_a_single_one_does_not() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let time = column_id(&state, sheet1, "Time");
    let signal = column_id(&state, sheet1, "Signal");
    let note = column_id(&state, sheet1, "Note");
    let root = state.tree().root();
    let last_graph = |state: &AppState| {
        state
            .tree()
            .node(root)
            .and_then(|node| node.children().last().copied())
            .expect("graph")
    };

    state.dispatch(Action::PlotColumns {
        sheet: sheet1,
        columns: vec![signal],
        plot_type: PlotType::Line,
    });
    let only = last_graph(&state);
    assert_eq!(
        state.plot_spec(only).unwrap().panels[0].series[0].group,
        None
    );

    state.dispatch(Action::PlotColumns {
        sheet: sheet1,
        columns: vec![time, signal, note],
        plot_type: PlotType::Line,
    });
    let graph = last_graph(&state);
    let panel = state.plot_spec(graph).unwrap().panels[0].clone();
    assert!(panel.series.len() >= 2);
    assert!(panel.series.iter().all(|s| s.group == Some(1)), "{panel:?}");

    // A single column added: no group; two together: a second one.
    state.dispatch(Action::AddSeriesToGraph {
        graph: only,
        panel: 0,
        columns: vec![note],
        plot_type: None,
    });
    let only_after = state.plot_spec(only).unwrap();
    assert!(only_after.panels[0]
        .series
        .iter()
        .all(|s| s.group.is_none()));
    state.dispatch(Action::AddSeriesToGraph {
        graph,
        panel: 0,
        columns: vec![signal, note],
        plot_type: None,
    });
    let after = state.plot_spec(graph).unwrap().panels[0].clone();
    let n = panel.series.len();
    assert!(after.series[..n].iter().all(|s| s.group == Some(1)));
    assert!(after.series[n..].iter().all(|s| s.group == Some(2)));
    assert_eq!(after.series.len(), n + 2);
}

// -- 22 ter. Drag rows to a graph (#166) --

/// "Kinetics" sheet: three columns measured at t = 0, 10 and 20 s (x'),
/// plus a column without x'. Returns (state, sheet, columns with x').
fn kinetics(dir: &Path) -> (Session, AppState, NodeId, Vec<NodeId>) {
    let mut project = Project::new("Kinetics").expect("project");
    let root = project.root();
    let book = project.add_workbook(root, Some("Book1")).expect("workbook");
    let sheet = project
        .tree()
        .child_by_name(book, "Sheet1")
        .expect("read")
        .expect("sheet");
    let mut add = |name: &str, values: &[f64]| {
        project
            .add_column(sheet, None, Some(name), Column::from_f64(values))
            .expect("column")
    };
    let a = add("A", &[1.0, 2.0, 3.0]);
    let b = add("B", &[4.0, 5.0, 6.0]);
    let c = add("C", &[7.0, 8.0, 9.0]);
    let _sans_x = add("D", &[0.0, 0.0, 0.0]);
    let (session, mut state) = start(project, &dir.join("projet.stf"));
    for (column, time) in [(a, "0"), (b, "10"), (c, "20")] {
        set_header_field(&mut state, column, HeaderField::XPrime, time);
    }
    (session, state, sheet, vec![a, b, c])
}

/// Writes the x' value of the column named `name` of `sheet`.
fn set_x_prime(state: &mut AppState, sheet: NodeId, name: &str, text: &str) {
    let column = column_id(state, sheet, name);
    set_header_field(state, column, HeaderField::XPrime, text);
}

/// A graph created from a Y column paired with an X column (hence "with X").
fn graph_with_x(state: &mut AppState, sheet: NodeId) -> NodeId {
    let root = state.tree().root();
    let x = column_id(state, sheet, "A");
    let y = column_id(state, sheet, "B");
    state.dispatch(Action::SetColumnRole {
        column: x,
        role: ColumnRole::X,
    });
    state.dispatch(Action::PlotColumns {
        sheet,
        columns: vec![y],
        plot_type: PlotType::Line,
    });
    state
        .tree()
        .node(root)
        .and_then(|node| node.children().last().copied())
        .expect("graph created")
}

/// The gesture is only offered for **whole rows** AND a sheet that has
/// an x' row: not without a selection, nor for columns, nor without x'.
#[test]
fn row_drag_payload_needs_full_rows_and_an_x_prime_row() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state, sheet, _) = kinetics(dir.path());
    assert_eq!(state.row_drag_payload(sheet), None, "no selection");

    state.select_column(sheet, column_id(&state, sheet, "A"));
    assert_eq!(state.row_drag_payload(sheet), None, "columns, not rows");

    state.select_row(sheet, 1);
    state.extend_row_selection(sheet, 2);
    let payload = state.row_drag_payload(sheet).expect("rows + x'");
    assert_eq!((payload.sheet, payload.rows.clone()), (sheet, vec![1, 2]));

    // Without any x' value, no more gesture.
    for column in ["A", "B", "C"] {
        set_x_prime(&mut state, sheet, column, "");
    }
    state.select_row(sheet, 1);
    assert_eq!(state.row_drag_payload(sheet), None, "without x'");

    // A cell selection (not whole rows) cannot be dragged either.
    set_x_prime(&mut state, sheet, "A", "1");
    state.select_cell(sheet, column_id(&state, sheet, "A"), 1);
    assert_eq!(state.row_drag_payload(sheet), None);
}

/// Graph with no determined X: no dialog, the main X axis; a row =
/// a series, whose points are the row values placed at x'. A single
/// undo step.
#[test]
fn dropping_rows_on_a_graph_without_x_uses_the_main_axis_with_no_dialog() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state, sheet, columns) = kinetics(dir.path());
    state.dispatch(Action::AddGraph);
    let root = state.tree().root();
    let graph = *state.tree().node(root).unwrap().children().last().unwrap();
    state.select_row(sheet, 0);
    state.extend_row_selection(sheet, 1);
    let payload = state.row_drag_payload(sheet).expect("loads");

    state.dispatch(drop_rows_action(&payload, graph, 0));

    assert_eq!(state.pending_row_drop(), None, "no question without X");
    let spec = state.plot_spec(graph).expect("spec");
    assert_eq!(spec.panels.len(), 1, "main axis: no new layer");
    let series = &spec.panels[0].series;
    assert_eq!(series.len(), 2, "one series per selected row");
    assert_eq!(
        series.iter().map(|s| s.row).collect::<Vec<_>>(),
        [Some(0), Some(1)]
    );
    assert_eq!(series[0].y, columns[0], "anchored on the first x' column");

    let resolved = stokesforge_core::resolve_spec(state.tree(), &spec);
    let first = &resolved.panels[0].series[0];
    // Row 0: A=1, B=4, C=7 at times 0, 10, 20 — column D, without x', is not one.
    assert_eq!(first.x.as_ref().unwrap().as_slice(), [0.0, 10.0, 20.0]);
    assert_eq!(first.y.as_ref().unwrap().as_slice(), [1.0, 4.0, 7.0]);
    let second = &resolved.panels[0].series[1];
    assert_eq!(second.y.as_ref().unwrap().as_slice(), [2.0, 5.0, 8.0]);

    state.dispatch(Action::Undo);
    assert!(state.plot_spec(graph).unwrap().panels[0].series.is_empty());
}

/// Graph that already has an X: the drop changes nothing before the answer;
/// "Create Axis" (default) adds a layer with its own X axis, **in the same
/// undo step**; "Use Present Axis" stays in the targeted layer; "Cancel"
/// abandons.
#[test]
fn dropping_rows_on_a_graph_with_x_asks_which_axis() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state, sheet, _) = kinetics(dir.path());
    let graph = graph_with_x(&mut state, sheet);
    let before = state.plot_spec(graph).expect("spec");
    state.select_row(sheet, 2);
    let payload = state.row_drag_payload(sheet).expect("loads");

    state.dispatch(drop_rows_action(&payload, graph, 0));
    assert_eq!(state.pending_row_drop(), Some((graph, 1)), "question asked");
    assert_eq!(state.plot_spec(graph).unwrap(), before, "nothing changed");

    // Cancel: no more question, nothing changed.
    state.dispatch(Action::ConfirmRowDrop(None));
    assert_eq!(state.pending_row_drop(), None);
    assert_eq!(state.plot_spec(graph).unwrap(), before);

    // New axis: an overlaid layer, X axis on top, a single Undo.
    state.dispatch(drop_rows_action(&payload, graph, 0));
    state.dispatch(Action::ConfirmRowDrop(Some(RowAxis::New)));
    assert_eq!(state.pending_row_drop(), None);
    let spec = state.plot_spec(graph).unwrap();
    assert_eq!(spec.panels.len(), 2, "one more layer");
    assert_eq!(
        spec.panels[0], before.panels[0],
        "the original layer is intact"
    );
    assert!(spec.panels[1]
        .overlay
        .is_some_and(|o| o.x_top && !o.y_right));
    assert_eq!(spec.panels[1].series.len(), 1);
    assert_eq!(spec.panels[1].series[0].row, Some(2));
    state.dispatch(Action::Undo);
    assert_eq!(
        state.plot_spec(graph).unwrap(),
        before,
        "a single step, layer included"
    );

    // Present axis: the series join the targeted layer, without a new layer.
    state.dispatch(drop_rows_action(&payload, graph, 0));
    state.dispatch(Action::ConfirmRowDrop(Some(RowAxis::Present)));
    let spec = state.plot_spec(graph).unwrap();
    assert_eq!(spec.panels.len(), 1);
    assert_eq!(spec.panels[0].series.len(), 2);
    assert_eq!(spec.panels[0].series[1].row, Some(2));
}

/// #396: a new overlay axis at the top takes the mirror that the cell displays
/// there, in the same undo step: one Undo gives both back. Here through the
/// "Create Axis" answer of a row drop.
#[test]
fn a_new_overlay_axis_switches_the_displayed_mirror_off_in_the_same_undo_step() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state, sheet, _) = kinetics(dir.path());
    let graph = graph_with_x(&mut state, sheet);
    let mut spec = state.plot_spec(graph).expect("spec");
    spec.panels.get_mut(0).unwrap().top.mirror = true;
    state.dispatch(Action::SetPlotSpec { graph, spec });
    let before = state.plot_spec(graph).expect("spec");
    assert!(
        before.panels[0].top.mirror,
        "precondition: mirror displayed"
    );
    state.select_row(sheet, 2);
    let payload = state.row_drag_payload(sheet).expect("loads");

    state.dispatch(drop_rows_action(&payload, graph, 0));
    state.dispatch(Action::ConfirmRowDrop(Some(RowAxis::New)));
    let spec = state.plot_spec(graph).unwrap();
    assert_eq!(spec.panels.len(), 2);
    assert!(!spec.panels[0].top.mirror, "the overlay axis wins");

    state.dispatch(Action::Undo);
    assert_eq!(state.plot_spec(graph).unwrap(), before, "both come back");
}

/// #396: the Y2 columns dropped on an existing graph create their overlay
/// layer; the mirror it takes goes off in the same undo step. (A new graph has
/// no mirror to take.)
#[test]
fn dropping_a_y2_takes_the_displayed_mirror_in_the_same_undo_step() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let mut project = Project::new("Y2Mirror").expect("project");
    let root = project.root();
    let book = project.add_workbook(root, Some("Book1")).expect("workbook");
    let sheet = project
        .tree()
        .child_by_name(book, "Sheet1")
        .expect("read")
        .expect("sheet");
    let mut add = |name: &str| {
        project
            .add_column(sheet, None, Some(name), Column::from_f64(&[1.0, 2.0, 3.0]))
            .expect("column")
    };
    let c: Vec<NodeId> = ["X", "Y", "X2", "Y2"].map(&mut add).to_vec();
    let (_session, mut state) = start(project, &dir.path().join("projet.stf"));
    for (column, role) in
        c.iter()
            .copied()
            .zip([ColumnRole::X, ColumnRole::Y, ColumnRole::X2, ColumnRole::Y2])
    {
        state.dispatch(Action::SetColumnRole { column, role });
    }
    state.dispatch(Action::PlotColumns {
        sheet,
        columns: vec![c[0], c[1]],
        plot_type: PlotType::Line,
    });
    let graph = state
        .tree()
        .node(root)
        .and_then(|node| node.children().last().copied())
        .expect("graph created");
    let mut spec = state.plot_spec(graph).expect("spec");
    spec.panels.get_mut(0).unwrap().top.mirror = true;
    spec.panels.get_mut(0).unwrap().right.mirror = true;
    state.dispatch(Action::SetPlotSpec { graph, spec });
    let before = state.plot_spec(graph).expect("spec");

    state.dispatch(Action::AddSeriesToGraph {
        graph,
        panel: 0,
        columns: vec![c[2], c[3]],
        plot_type: None,
    });
    let spec = state.plot_spec(graph).expect("spec");
    assert_eq!(spec.panels.len(), 2, "the Y2 layer");
    assert!(!spec.panels[0].top.mirror && !spec.panels[0].right.mirror);

    state.dispatch(Action::Undo);
    assert_eq!(state.plot_spec(graph).unwrap(), before, "both come back");
}

/// #396: same rule for "New Layer": the mirror goes off only on a side the new
/// layer really draws (a linked scale draws nothing).
#[test]
fn add_layer_takes_the_mirror_only_where_it_draws_an_axis() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let signal = column_id(&state, sheet1, "Signal");
    let root = state.tree().root();
    state.dispatch(Action::PlotColumns {
        sheet: sheet1,
        columns: vec![signal],
        plot_type: PlotType::Line,
    });
    let graph = state
        .tree()
        .node(root)
        .and_then(|node| node.children().last().copied())
        .expect("graph created");
    let mut spec = state.plot_spec(graph).expect("spec");
    spec.panels.get_mut(0).unwrap().top.mirror = true;
    spec.panels.get_mut(0).unwrap().right.mirror = true;
    state.dispatch(Action::SetPlotSpec { graph, spec });
    let before = state.plot_spec(graph).expect("spec");

    // X on top only: the top mirror goes, the right one stays.
    state.dispatch(Action::AddLayer {
        graph,
        panel: 0,
        x_top: true,
        y_right: false,
    });
    let spec = state.plot_spec(graph).expect("spec");
    assert!(!spec.panels[0].top.mirror && spec.panels[0].right.mirror);
    state.dispatch(Action::Undo);
    assert_eq!(state.plot_spec(graph).unwrap(), before, "one Undo");

    // Linked scales: no axis drawn, the mirrors stay.
    state.dispatch(Action::AddLayer {
        graph,
        panel: 0,
        x_top: false,
        y_right: false,
    });
    let spec = state.plot_spec(graph).expect("spec");
    assert!(spec.panels[0].top.mirror && spec.panels[0].right.mirror);
}

/// #396: the address of an axis is a field of its layer; Graph Properties
/// opens on the side where the axis is drawn.
#[test]
fn opening_the_properties_of_an_overlay_axis_selects_the_side_it_is_drawn_on() {
    use stokesforge_app_state::object_manager::{AxisRef, VisibilityTarget as T};
    use stokesforge_plot::{AxisSide, OverlaySpec, PanelSpec, Panels, PlotSpec};
    let dir = tempfile::tempdir().expect("temporary directory");
    let (project, graph, _) = object_manager_project();
    let (_session, mut state) = start(project, &dir.path().join("sides.stf"));
    let base = PanelSpec::at(0, 0);
    let spec = PlotSpec::empty("Sides").with_panels(Panels::new(vec![
        base.clone(),
        PanelSpec::overlay_on(&base, OverlaySpec::new(true, true)),
    ]));
    state.dispatch(Action::SetPlotSpec { graph, spec });
    for (panel, side, opened) in [
        (1, AxisSide::Bottom, PlotAxis::Top),
        (1, AxisSide::Left, PlotAxis::Right),
        (0, AxisSide::Bottom, PlotAxis::Bottom),
        (0, AxisSide::Left, PlotAxis::Left),
    ] {
        state.dispatch(Action::OpenObjectProperties {
            graph,
            target: T::Axis(AxisRef { panel, side }),
        });
        let settings = state.plot_settings().expect("window open");
        assert_eq!(settings.axis, opened, "layer {panel}, field {side:?}");
        assert_eq!(state.plot_edit().unwrap().selected_panel, panel);
    }
}

/// A row outside the sheet is ignored; a sheet without x' refuses with
/// a message, writing nothing.
#[test]
fn dropping_rows_ignores_missing_rows_and_refuses_a_sheet_without_x_prime() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state, sheet, _) = kinetics(dir.path());
    state.dispatch(Action::AddGraph);
    let root = state.tree().root();
    let graph = *state.tree().node(root).unwrap().children().last().unwrap();

    state.dispatch(Action::AddRowsToGraph {
        graph,
        panel: 0,
        sheet,
        rows: vec![1, 99],
        axis: RowAxis::Ask,
    });
    let series = &state.plot_spec(graph).unwrap().panels[0].series;
    assert_eq!(series.len(), 1);
    assert_eq!(series[0].row, Some(1));

    for column in ["A", "B", "C"] {
        set_x_prime(&mut state, sheet, column, "");
    }
    let before = state.plot_spec(graph).unwrap();
    state.dispatch(Action::AddRowsToGraph {
        graph,
        panel: 0,
        sheet,
        rows: vec![0],
        axis: RowAxis::Ask,
    });
    assert_eq!(state.plot_spec(graph).unwrap(), before);
    assert!(state.status().expect("message").is_error());
}

/// Plot of a row selection (#166): a fresh graph where each row is
/// a series, not all the columns crossed; a single undo step.
#[test]
fn plot_rows_creates_a_graph_with_one_row_series_per_row() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state, sheet, columns) = kinetics(dir.path());
    let root = state.tree().root();
    let before = state.tree().len();

    state.dispatch(Action::PlotRows {
        sheet,
        rows: vec![2, 0, 99],
        plot_type: PlotType::Line,
    });

    let graph = state
        .tree()
        .node(root)
        .and_then(|node| node.children().last().copied())
        .expect("graph created");
    let spec = state.plot_spec(graph).expect("spec");
    let series = &spec.panels[0].series;
    assert_eq!(
        series.iter().map(|s| s.row).collect::<Vec<_>>(),
        [Some(0), Some(2)]
    );
    assert_eq!(series[0].y, columns[0]);
    assert!(series.iter().all(|s| s.x.is_none()));
    state.dispatch(Action::Undo);
    assert_eq!(state.tree().len(), before);

    // Without x', nothing is created.
    for name in ["A", "B", "C"] {
        set_x_prime(&mut state, sheet, name, "");
    }
    state.dispatch(Action::PlotRows {
        sheet,
        rows: vec![0],
        plot_type: PlotType::Line,
    });
    assert_eq!(state.tree().len(), before);
    assert!(state.status().expect("message").is_error());
}

// -- 23. Drag a column selection to a graph --

/// [`AppState::column_drag_payload`] returns `None` as long as `sheet` has no
/// selection covering a column: no drag possible without a column,
/// even from the grab zone.
#[test]
fn column_drag_payload_is_none_without_a_column_selection() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");

    assert_eq!(state.column_drag_payload(sheet1), None);
}

/// Only a selection of **whole columns** can be dragged (#160): cells
/// or rows, even covering columns, do not offer the
/// gesture.
#[test]
fn column_drag_payload_needs_whole_columns() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let time = column_id(&state, sheet1, "Time");
    let signal = column_id(&state, sheet1, "Signal");

    state.select_cell(sheet1, time, 0);
    state.extend_selection(sheet1, signal, 1);
    assert_eq!(state.column_drag_payload(sheet1), None, "cells");

    state.select_row(sheet1, 1);
    assert_eq!(state.column_drag_payload(sheet1), None, "a row");

    state.select_header_row(sheet1, HeaderField::Unit);
    assert_eq!(state.column_drag_payload(sheet1), None, "a header field");
}

/// With a selection of whole columns, the payload carries the sheet
/// and **all** the selected columns in sheet order, even
/// non-contiguous ones, which `stokesforge_app::app::grid::grid_contents` drags from
/// the block edge and which [`drop_payload_action`] reads back at the drop.
#[test]
fn column_drag_payload_carries_the_sheet_and_its_selected_columns() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let time = column_id(&state, sheet1, "Time");
    let signal = column_id(&state, sheet1, "Signal");
    let note = column_id(&state, sheet1, "Note");

    state.select_column(sheet1, signal);
    state.extend_column_selection(sheet1, time);
    let payload = state
        .column_drag_payload(sheet1)
        .expect("selection covering Time and Signal");
    assert_eq!(payload.sheet, sheet1);
    assert_eq!(payload.columns, vec![time, signal]);

    state.select_column(sheet1, note);
    state.toggle_column_selection(sheet1, time);
    assert_eq!(
        state.column_drag_payload(sheet1).unwrap().columns,
        vec![time, note],
        "non-contiguous, in sheet order"
    );

    state.select_all(sheet1);
    assert_eq!(
        state.column_drag_payload(sheet1).unwrap().columns,
        vec![time, signal, note],
        "the corner cell takes them all"
    );
}

/// [`drop_payload_action`] targets the given layer and **does not impose** a
/// plot type (`None`): the series take that of the layer (#160).
#[test]
fn drop_payload_action_targets_the_panel_without_forcing_a_plot_type() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let signal = column_id(&state, sheet1, "Signal");
    let graph = NodeId::from_raw(999);
    let payload = ColumnDragPayload {
        sheet: sheet1,
        columns: vec![signal],
    };

    assert_eq!(
        drop_payload_action(&payload, graph, 2),
        Action::AddSeriesToGraph {
            graph,
            panel: 2,
            columns: vec![signal],
            plot_type: None,
        }
    );
}

/// Plots `Signal` as a scatter, drops `Note` into it: the new series
/// takes the type of the layer (scatter), not `Line`. A single undo step.
#[test]
fn a_dropped_series_takes_the_plot_type_of_the_target_panel() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let signal = column_id(&state, sheet1, "Signal");
    let note = column_id(&state, sheet1, "Note");
    let root = state.tree().root();

    state.dispatch(Action::PlotColumns {
        sheet: sheet1,
        columns: vec![signal],
        plot_type: PlotType::Scatter,
    });
    let graph = state
        .tree()
        .node(root)
        .and_then(|node| node.children().last().copied())
        .expect("the graph was created");
    let before = state.plot_spec(graph).unwrap();

    state.select_column(sheet1, note);
    let payload = state.column_drag_payload(sheet1).expect("Note selected");
    state.dispatch(drop_payload_action(&payload, graph, 0));

    let spec = state.plot_spec(graph).expect("spec after drop");
    assert_eq!(spec.panels[0].series.len(), 2);
    assert_eq!(spec.panels[0].series[1].y, note);
    assert_eq!(spec.panels[0].series[1].plot_type, PlotType::Scatter);

    state.dispatch(Action::Undo);
    assert_eq!(state.plot_spec(graph).unwrap(), before, "a single step");
}

/// A layer that mixes several types takes that of its last series;
/// an empty layer, `Line`; a nonexistent layer adds nothing.
#[test]
fn the_drop_plot_type_is_the_last_series_type_or_line_for_an_empty_panel() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let signal = column_id(&state, sheet1, "Signal");
    let note = column_id(&state, sheet1, "Note");
    let root = state.tree().root();

    state.dispatch(Action::AddGraph);
    let graph = state
        .tree()
        .node(root)
        .and_then(|node| node.children().last().copied())
        .expect("empty graph");
    state.dispatch(Action::AddSeriesToGraph {
        graph,
        panel: 0,
        columns: vec![signal],
        plot_type: None,
    });
    assert_eq!(
        state.plot_spec(graph).unwrap().panels[0].series[0].plot_type,
        PlotType::Line,
        "empty layer: Line"
    );

    state.dispatch(Action::AddSeriesToGraph {
        graph,
        panel: 0,
        columns: vec![note],
        plot_type: Some(PlotType::Bar),
    });
    state.dispatch(Action::AddSeriesToGraph {
        graph,
        panel: 0,
        columns: vec![signal],
        plot_type: None,
    });
    let series = &state.plot_spec(graph).unwrap().panels[0].series;
    assert_eq!(series.len(), 3);
    assert_eq!(series[2].plot_type, PlotType::Bar, "last series: Bar");

    let before = state.plot_spec(graph).unwrap();
    state.dispatch(Action::AddSeriesToGraph {
        graph,
        panel: 7,
        columns: vec![signal],
        plot_type: None,
    });
    assert_eq!(state.plot_spec(graph).unwrap(), before, "unknown layer");
}

// -- 24. Graph properties --

/// [`active_graph`] decides the presence of the "Graph" menu: only
/// `ActiveTabKind::Graph` returns `Some`, all the other kinds of tab
/// (or no active tab) return `None`.
#[test]
fn active_graph_is_some_only_for_a_graph_tab() {
    let graph = NodeId::from_raw(7);
    assert_eq!(active_graph(ActiveTabKind::Graph(graph)), Some(graph));
    assert_eq!(active_graph(ActiveTabKind::Book(graph)), None);
    assert_eq!(active_graph(ActiveTabKind::Matrix(graph)), None);
    assert_eq!(active_graph(ActiveTabKind::Note(graph)), None);
    assert_eq!(active_graph(ActiveTabKind::None), None);
}

/// Plots `Signal` of `Sheet1` into a new graph and returns it.
fn plotted_graph(state: &mut AppState) -> NodeId {
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let signal = column_id(state, sheet1, "Signal");
    let root = state.tree().root();
    state.dispatch(Action::PlotColumns {
        sheet: sheet1,
        columns: vec![signal],
        plot_type: PlotType::Line,
    });
    state
        .tree()
        .node(root)
        .and_then(|node| node.children().last().copied())
        .expect("the graph was created")
}

/// A right-click on a component opens the tab that sets it: title ->
/// "General", plot area -> "Plot Area", axis or axis title ->
/// "Axes" on that axis.
#[test]
fn a_right_clicked_component_opens_the_settings_page_that_sets_it() {
    // Compared in English: the test must not depend on `STOKESFORGE_LANG` or on the
    // saved preference of whoever runs it.
    stokesforge_app_state::i18n::set_language(stokesforge_app_state::i18n::Language::English);
    assert_eq!(
        PlotTarget::Title.settings_page(),
        (PlotSettingsTab::Graph, None)
    );
    assert_eq!(
        PlotTarget::Background.settings_page(),
        (PlotSettingsTab::Graph, None)
    );
    assert_eq!(
        PlotTarget::PlotArea.settings_page(),
        (PlotSettingsTab::PlotArea, None)
    );
    assert_eq!(
        PlotTarget::Axis(PlotAxis::Left).settings_page(),
        (PlotSettingsTab::Axes, Some(PlotAxis::Left))
    );
    assert_eq!(
        PlotTarget::AxisTitle(PlotAxis::Bottom).settings_page(),
        (PlotSettingsTab::Axes, Some(PlotAxis::Bottom))
    );
    assert_eq!(
        PlotTarget::AxisTitle(PlotAxis::Bottom).label(),
        "Bottom Axis Title"
    );
}

/// `Action::OpenPlotSettings` opens the window on the requested tab and axis
/// and a draft of the graph; with no axis given ("Graph" menu), the
/// axis already chosen is kept.
#[test]
fn open_plot_settings_opens_the_requested_tab_and_keeps_the_chosen_axis() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let graph = plotted_graph(&mut state);
    assert!(state.plot_settings().is_none());

    state.dispatch(PlotTarget::Axis(PlotAxis::Left).open_settings(graph));
    let settings = state.plot_settings().expect("window open");
    assert_eq!(settings.graph, graph);
    assert_eq!(settings.tab, PlotSettingsTab::Axes);
    assert_eq!(settings.axis, PlotAxis::Left);
    assert_eq!(state.plot_edit().expect("draft").graph, graph);

    state.dispatch(Action::OpenPlotSettings {
        graph,
        tab: PlotSettingsTab::Legend,
        axis: None,
    });
    let settings = state.plot_settings().expect("window open");
    assert_eq!(settings.tab, PlotSettingsTab::Legend);
    assert_eq!(settings.axis, PlotAxis::Left, "the chosen axis is kept");
}

/// A node that is not a graph opens nothing.
#[test]
fn open_plot_settings_ignores_a_node_that_is_not_a_graph() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    state.dispatch(Action::OpenPlotSettings {
        graph: sheet1,
        tab: PlotSettingsTab::Graph,
        axis: None,
    });
    assert!(state.plot_settings().is_none());
    assert!(state.plot_edit().is_none());
}

/// Closing the window validates an input still in progress (field that has not
/// lost focus), then abandons the draft.
#[test]
fn closing_plot_settings_commits_a_pending_edit() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let graph = plotted_graph(&mut state);
    state.dispatch(PlotTarget::Title.open_settings(graph));
    state.plot_edit_mut().expect("draft").spec.title = "Spectrum".to_string();

    state.close_plot_settings();

    assert!(state.plot_settings().is_none());
    assert!(state.plot_edit().is_none());
    assert_eq!(state.plot_spec(graph).expect("spec").title, "Spectrum");
}

/// #15: "Cancel" (also the cross and Esc) puts the graph back in the state it
/// was in when the window opened, after a live preview, and leaves
/// no history entry.
#[test]
fn cancelling_plot_settings_restores_the_graph_and_leaves_no_history() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let graph = plotted_graph(&mut state);
    let original = state.plot_spec(graph).expect("spec");
    let label_before = state.undo_label();
    state.dispatch(PlotTarget::Title.open_settings(graph));

    state.plot_edit_mut().expect("draft").spec.title = "Preview".to_string();
    state.commit_plot_edit();
    assert_eq!(
        state.plot_spec(graph).expect("spec").title,
        "Preview",
        "live preview in the model"
    );

    state.cancel_plot_settings();

    assert!(state.plot_settings().is_none());
    assert!(state.plot_edit().is_none());
    assert_eq!(state.plot_spec(graph).expect("spec"), original);
    assert_eq!(state.undo_label(), label_before, "no entry added");
}

/// #15: "OK" keeps everything, in **a single** history entry: one
/// Ctrl+Z undoes all the window's gestures at once.
#[test]
fn accepting_plot_settings_is_a_single_history_entry() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let graph = plotted_graph(&mut state);
    let original = state.plot_spec(graph).expect("spec");
    state.dispatch(PlotTarget::Title.open_settings(graph));

    state.plot_edit_mut().expect("draft").spec.title = "First".to_string();
    state.commit_plot_edit();
    state.plot_edit_mut().expect("draft").spec.background = "#102030".to_string();
    state.commit_plot_edit();
    state.close_plot_settings();

    let kept = state.plot_spec(graph).expect("spec");
    assert_eq!(kept.title, "First");
    assert_eq!(kept.background, "#102030");

    state.dispatch(Action::Undo);

    assert_eq!(
        state.plot_spec(graph).expect("spec"),
        original,
        "a single Ctrl+Z undoes both gestures"
    );
}

/// Opening the properties of another graph first validates the draft of the
/// first: nothing is lost going from one to the other.
#[test]
fn switching_plot_settings_to_another_graph_commits_the_first_draft() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let first = plotted_graph(&mut state);
    let second = plotted_graph(&mut state);
    assert_ne!(first, second);
    state.dispatch(PlotTarget::Title.open_settings(first));
    state.plot_edit_mut().expect("draft").spec.title = "First".to_string();

    state.dispatch(PlotTarget::Title.open_settings(second));

    assert_eq!(state.plot_spec(first).expect("spec").title, "First");
    assert_eq!(state.plot_edit().expect("draft").graph, second);
}

/// A draft with no input in progress follows the model: after Cancel, the
/// window shows the restored spec, not the old one, otherwise the next
/// validation would rewrite over the undo.
#[test]
fn an_idle_plot_draft_follows_undo() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let graph = plotted_graph(&mut state);
    let original = state.plot_spec(graph).expect("spec").title;
    // Draft opened without the window (no history macro: Cancel
    // stays possible, unlike the modal window which refuses it).
    state.begin_plot_edit(graph);
    state.plot_edit_mut().expect("draft").spec.title = "Modified".to_string();
    state.commit_plot_edit();

    state.dispatch(Action::Undo);
    state.begin_plot_edit(graph);

    assert_eq!(state.plot_spec(graph).expect("spec").title, original);
    assert_eq!(
        state.plot_edit().expect("draft").spec.title,
        original,
        "the draft followed the undo"
    );
}

/// `AppState::begin_plot_edit` opens a draft initialized on the current spec
/// of the graph, and `AppState::commit_plot_edit` sends it to the model
/// (`Action::SetPlotSpec`): a full round trip, as the panel would do
/// when editing the graph title.
#[test]
fn begin_and_commit_plot_edit_round_trip_through_set_plot_spec() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let signal = column_id(&state, sheet1, "Signal");
    let root = state.tree().root();
    state.dispatch(Action::PlotColumns {
        sheet: sheet1,
        columns: vec![signal],
        plot_type: PlotType::Line,
    });
    let graph = state
        .tree()
        .node(root)
        .and_then(|node| node.children().last().copied())
        .expect("the graph was created");

    state.begin_plot_edit(graph);
    let edit = state.plot_edit().expect("draft open").clone();
    assert_eq!(edit.graph, graph);
    assert_eq!(edit.spec, state.plot_spec(graph).expect("initial spec"));

    state.plot_edit_mut().expect("draft").spec.title = "My figure".to_string();
    state.commit_plot_edit();

    let after = state.plot_spec(graph).expect("spec after commit");
    assert_eq!(after.title, "My figure");
}

/// Validating a draft that has **not** changed sends no command, since
/// otherwise losing focus on an unmodified text field, or releasing a
/// cursor that did not move, would stack a useless history entry.
#[test]
fn committing_an_unchanged_plot_edit_sends_no_command() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let signal = column_id(&state, sheet1, "Signal");
    let root = state.tree().root();
    state.dispatch(Action::PlotColumns {
        sheet: sheet1,
        columns: vec![signal],
        plot_type: PlotType::Line,
    });
    let graph = state
        .tree()
        .node(root)
        .and_then(|node| node.children().last().copied())
        .expect("the graph was created");

    state.begin_plot_edit(graph);
    let undo_label_before = state.undo_label();

    state.commit_plot_edit();

    assert_eq!(
        state.undo_label(),
        undo_label_before,
        "draft identical to the current spec: nothing to validate"
    );
}

/// Ctrl+Z after a panel validation restores the previous spec, like
/// any other undoable command: the panel has no mutation path of its
/// own, it goes through `Command::SetPlotSpec` like the others.
#[test]
fn undo_after_committing_a_plot_edit_restores_the_previous_spec() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let signal = column_id(&state, sheet1, "Signal");
    let root = state.tree().root();
    state.dispatch(Action::PlotColumns {
        sheet: sheet1,
        columns: vec![signal],
        plot_type: PlotType::Line,
    });
    let graph = state
        .tree()
        .node(root)
        .and_then(|node| node.children().last().copied())
        .expect("the graph was created");
    let before = state.plot_spec(graph).expect("initial spec");

    state.begin_plot_edit(graph);
    state.plot_edit_mut().expect("draft").spec.title = "Modified title".to_string();
    state.commit_plot_edit();
    assert_ne!(state.plot_spec(graph).expect("spec after commit"), before);

    state.dispatch(Action::Undo);

    assert_eq!(state.plot_spec(graph).expect("spec after undo"), before);
}

/// Repainting the same frame on the same graph must never erase an input
/// in progress: `begin_plot_edit` is a no-op as long as the targeted graph
/// has not changed, even called several times in a row.
#[test]
fn begin_plot_edit_does_not_clobber_an_in_progress_draft() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let signal = column_id(&state, sheet1, "Signal");
    let root = state.tree().root();
    state.dispatch(Action::PlotColumns {
        sheet: sheet1,
        columns: vec![signal],
        plot_type: PlotType::Line,
    });
    let graph = state
        .tree()
        .node(root)
        .and_then(|node| node.children().last().copied())
        .expect("the graph was created");

    state.begin_plot_edit(graph);
    state.plot_edit_mut().expect("draft").spec.title = "Being typed".to_string();

    // Repeats the call, as `plot_settings_window` would on every frame.
    state.begin_plot_edit(graph);
    state.begin_plot_edit(graph);

    assert_eq!(
        state.plot_edit().expect("draft still open").spec.title,
        "Being typed",
        "the draft being typed was not erased"
    );
}

/// Changing graph resets the draft on the spec of the other graph, with no
/// residue of the previous edit displayed on the wrong
/// graph.
#[test]
fn begin_plot_edit_resets_the_draft_when_switching_graphs() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let signal = column_id(&state, sheet1, "Signal");
    let note = column_id(&state, sheet1, "Note");
    let root = state.tree().root();

    state.dispatch(Action::PlotColumns {
        sheet: sheet1,
        columns: vec![signal],
        plot_type: PlotType::Line,
    });
    let first_graph = state
        .tree()
        .node(root)
        .and_then(|node| node.children().last().copied())
        .expect("first graph created");
    state.dispatch(Action::PlotColumns {
        sheet: sheet1,
        columns: vec![note],
        plot_type: PlotType::Scatter,
    });
    let second_graph = state
        .tree()
        .node(root)
        .and_then(|node| node.children().last().copied())
        .expect("second graph created");
    assert_ne!(first_graph, second_graph);

    state.begin_plot_edit(first_graph);
    state.plot_edit_mut().expect("draft").spec.title = "Residue".to_string();

    state.begin_plot_edit(second_graph);

    let edit = state.plot_edit().expect("draft of the second graph");
    assert_eq!(edit.graph, second_graph);
    assert_ne!(
        edit.spec.title, "Residue",
        "the draft of the first graph must not leak onto the second"
    );
}

/// A series whose plot type is [`PlotType::Unknown`] (C5, variant
/// of a more recent StokesForge version than this reader knows) must be
/// openable in the edit draft and shown in the
/// series list without ever panicking: invariant 10 ("a data defect
/// degrades, it does not crash"), which also holds for this panel.
#[test]
fn a_series_with_an_unknown_plot_type_can_be_opened_without_panicking() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let signal = column_id(&state, sheet1, "Signal");
    let root = state.tree().root();
    state.dispatch(Action::PlotColumns {
        sheet: sheet1,
        columns: vec![signal],
        plot_type: PlotType::Line,
    });
    let graph = state
        .tree()
        .node(root)
        .and_then(|node| node.children().last().copied())
        .expect("the graph was created");

    state.begin_plot_edit(graph);
    {
        let edit = state.plot_edit_mut().expect("draft");
        edit.spec.panels.get_mut(0).expect("panel").series[0].plot_type =
            PlotType::Unknown(RawVariant {
                name: "FutureType".to_string(),
                payload: vec![1, 2, 3],
            });
    }
    state.commit_plot_edit();

    let spec = state.plot_spec(graph).expect("spec after commit");
    assert!(matches!(
        spec.panels[0].series[0].plot_type,
        PlotType::Unknown(_)
    ));
}

/// The scale of an axis is edited properly through the draft: this test serves
/// as a guard for any `AxisSpec` field edited by the panel: if
/// a future field rename breaks the direct binding of the widgets, a test
/// that no longer goes through `PlotEdit` at all would not see it.
#[test]
fn editing_an_axis_scale_through_the_draft_is_visible_after_commit() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let signal = column_id(&state, sheet1, "Signal");
    let root = state.tree().root();
    state.dispatch(Action::PlotColumns {
        sheet: sheet1,
        columns: vec![signal],
        plot_type: PlotType::Line,
    });
    let graph = state
        .tree()
        .node(root)
        .and_then(|node| node.children().last().copied())
        .expect("the graph was created");

    state.begin_plot_edit(graph);
    {
        let edit: &mut PlotEdit = state.plot_edit_mut().expect("draft");
        let panel = edit.spec.panels.get_mut(0).expect("panel");
        panel.left.scale = AxisScale::Log;
        panel.left.auto_range = false;
        panel.left.range_min = Some(0.1);
        panel.left.range_max = Some(100.0);
    }
    state.commit_plot_edit();

    let spec = state.plot_spec(graph).expect("spec after commit");
    assert_eq!(spec.panels[0].left.scale, AxisScale::Log);
    assert!(spec.panels[0].left.has_explicit_range());
    assert_eq!(spec.panels[0].left.range_min, Some(0.1));
    assert_eq!(spec.panels[0].left.range_max, Some(100.0));
}

// -- 25. Saved colors --

/// Saving a color "in the project" goes through an undoable
/// command: it appears in the tree, Cancel removes it.
#[test]
fn saving_a_color_in_the_project_is_undoable() {
    use stokesforge_app_state::state::SaveTarget;
    use stokesforge_plot::ColorUsage;
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    state.use_user_colors_file(dir.path().join("couleurs.json"));

    state.dispatch(Action::SaveColor {
        usage: ColorUsage::Curves,
        color: "#FF0000".to_string(),
        target: SaveTarget::Project,
    });
    assert_eq!(
        state
            .saved_colors(SaveTarget::Project)
            .get(ColorUsage::Curves),
        ["#ff0000"]
    );
    assert!(
        state
            .saved_colors(SaveTarget::Project)
            .get(ColorUsage::Text)
            .is_empty(),
        "filed by use"
    );
    assert!(
        state.saved_colors(SaveTarget::User).is_empty(),
        "not with the user"
    );

    state.dispatch(Action::Undo);
    assert!(state.saved_colors(SaveTarget::Project).is_empty());
}

/// Saving a "user" color writes it to the user's file,
/// outside the project and outside the history; removing it too.
#[test]
fn saving_a_color_for_the_user_writes_the_user_file() {
    use stokesforge_app_state::state::SaveTarget;
    use stokesforge_plot::ColorUsage;
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let path = dir.path().join("config").join("couleurs.json");
    state.use_user_colors_file(path.clone());

    state.dispatch(Action::SaveColor {
        usage: ColorUsage::Background,
        color: "#ffffff00".to_string(),
        target: SaveTarget::User,
    });
    assert_eq!(
        state
            .saved_colors(SaveTarget::User)
            .get(ColorUsage::Background),
        ["#ffffff00"]
    );
    assert!(state.saved_colors(SaveTarget::Project).is_empty());
    assert!(!state.can_undo(), "outside the history");
    let written = std::fs::read_to_string(&path).expect("file written");
    assert!(written.contains("#ffffff00"));

    state.dispatch(Action::RemoveSavedColor {
        usage: ColorUsage::Background,
        color: "#FFFFFF00".to_string(),
        target: SaveTarget::User,
    });
    assert!(state.saved_colors(SaveTarget::User).is_empty());
}

/// #178: a color gradient is stored in the project (undoable) or with the user
/// (file, outside the history), replaced by its name and deleted.
#[test]
fn gradients_are_saved_in_the_project_or_with_the_user_and_deleted() {
    use stokesforge_app_state::state::SaveTarget;
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let path = dir.path().join("config").join("couleurs.json");
    state.use_user_colors_file(path.clone());
    let stops = vec!["#112233".to_string(), "#aabbcc".to_string()];

    state.dispatch(Action::SaveGradient {
        name: "My gradient".to_string(),
        stops: stops.clone(),
        target: SaveTarget::Project,
    });
    let project = state.saved_colors(SaveTarget::Project).gradients;
    assert_eq!(project.len(), 1);
    assert_eq!(project[0].name, "My gradient");
    assert_eq!(project[0].stops, stops);
    assert!(state.saved_colors(SaveTarget::User).gradients.is_empty());
    state.dispatch(Action::Undo);
    assert!(state.saved_colors(SaveTarget::Project).gradients.is_empty());

    state.dispatch(Action::SaveGradient {
        name: "Perso".to_string(),
        stops,
        target: SaveTarget::User,
    });
    assert_eq!(state.saved_colors(SaveTarget::User).gradients.len(), 1);
    assert!(!state.can_undo(), "outside the history");
    assert!(std::fs::read_to_string(&path)
        .expect("file written")
        .contains("Perso"));
    // A gradient with the same name (case-insensitive) is replaced, not duplicated.
    state.dispatch(Action::SaveGradient {
        name: "perso".to_string(),
        stops: vec!["#000000".to_string(), "#ffffff".to_string()],
        target: SaveTarget::User,
    });
    assert_eq!(state.saved_colors(SaveTarget::User).gradients.len(), 1);

    state.dispatch(Action::RemoveSavedGradient {
        name: "PERSO".to_string(),
        target: SaveTarget::User,
    });
    assert!(state.saved_colors(SaveTarget::User).gradients.is_empty());
}

/// A graph created from the workbook has no background: "no color"
/// (transparent) is the default background.
#[test]
fn a_new_graph_has_no_background_color() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let graph = plotted_graph(&mut state);
    assert_eq!(
        state.plot_spec(graph).expect("spec").background,
        stokesforge_plot::TRANSPARENT
    );
}

// -- File drag and drop (issue #8) --

/// An OS drop can arrive from a detached viewport despite the root's modal
/// dialog: it must not enter the graph's macro.
#[test]
fn dropping_files_during_plot_settings_is_refused_without_touching_the_macro() {
    let _recipes = isolate_recipe_store();
    let dir = tempfile::tempdir().unwrap();
    let (session, mut state) = start_demo(dir.path());
    let graph = plotted_graph(&mut state);
    state.dispatch(Action::OpenPlotSettings {
        graph,
        tab: PlotSettingsTab::Graph,
        axis: None,
    });
    let before = session.handle().snapshot().unwrap();
    assert!(before.history.in_macro, "the dialog opens its macro");
    let file = dir.path().join("mesure.csv");
    std::fs::write(&file, "1,10\n2,20\n").unwrap();
    state.dispatch(Action::DropFiles {
        paths: vec![file.clone()],
        target_book: None,
    });
    let after = session.handle().snapshot().unwrap();
    assert_eq!(*after.tree, *before.tree);
    assert_eq!(after.revision, before.revision);
    assert!(after.history.in_macro, "the dialog keeps its macro");
    // S-M7 (#228): the refusal is written in the footer of the open dialog.
    assert!(state.dialog_error().is_some());
    assert!(!state.can_reopen_last_import());
    state.cancel_plot_settings();
    state.dispatch(Action::DropFiles {
        paths: vec![file],
        target_book: None,
    });
    wait_import(&mut state);
    assert!(
        !state.status().unwrap().is_error(),
        "the drop goes through after the dialog closes"
    );
    assert!(state.can_reopen_last_import());
}

/// The last model really used takes precedence over the catalog order.
/// A drop always creates new sheets, even after a replacement.
#[test]
fn dropping_files_uses_the_last_recipe_and_always_creates_new_sheets() {
    let _recipes = isolate_recipe_store();
    let dir = tempfile::tempdir().unwrap();
    let (_session, mut state) = start_demo(dir.path());
    let book = book_named(state.tree(), "Book1");
    let mut recipe = stokesforge_import::ImportRecipe::new("Last template used");
    recipe.separator = stokesforge_import::Separator::Semicolon;
    recipe.has_column_header_row = true;
    recipe.import_mode = stokesforge_import::ImportMode::ReplaceSheet;
    recipe.files_layout = stokesforge_import::FilesLayout::SideBySide;
    let initial = dir.path().join("initial.csv");
    std::fs::write(&initial, "Time;Signal\n1;10\n").unwrap();
    state.dispatch(Action::ImportFiles {
        destination: ImportDestination::ActiveBook(book),
        recipe,
        paths: vec![initial],
    });
    assert!(!state.status().unwrap().is_error());
    let before = state.snapshot().tree.clone();
    let sheets_before = state.worksheets_of(book).len();
    let files: Vec<_> = ["drop1.csv", "drop2.csv"]
        .into_iter()
        .map(|name| {
            let path = dir.path().join(name);
            std::fs::write(&path, "Time;Signal\n2;20\n3;30\n").unwrap();
            path
        })
        .collect();
    state.dispatch(Action::DropFiles {
        paths: files,
        target_book: Some(book),
    });
    wait_import(&mut state);
    assert_eq!(state.worksheets_of(book).len(), sheets_before + 2);
    let sheet = sheet_named(state.tree(), "drop1");
    let columns = state.visible_columns(sheet);
    assert_eq!(state.column_meta(columns[0].0).long_name, "Time");
    assert_eq!(cell_f64(&state, columns[0].0, 0), 2.);
    assert!(state.status().unwrap().text.contains("Last template used"));
    state.dispatch(Action::Undo);
    assert_eq!(*state.snapshot().tree, *before);
}

/// With no last import or recognized template, the permissive recipe is used,
/// never the first template of the catalog, which could swallow the data.
#[test]
fn dropping_an_unknown_file_without_history_uses_the_default_recipe() {
    let _recipes = isolate_recipe_store();
    let mut store = stokesforge_import::RecipeStore::default();
    let mut unsuitable = stokesforge_import::ImportRecipe::new("Bad template");
    unsuitable.header = stokesforge_import::HeaderSkip::Lines(1);
    store.upsert(unsuitable);
    store.save().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let (_session, mut state) = start_demo(dir.path());
    let book = book_named(state.tree(), "Book1");
    let path = dir.path().join("inconnu.csv");
    std::fs::write(&path, "1,10\n2,20\n").unwrap();
    state.dispatch(Action::DropFiles {
        paths: vec![path],
        target_book: Some(book),
    });
    wait_import(&mut state);
    let sheet = sheet_named(state.tree(), "inconnu");
    let x = state.visible_columns(sheet)[0].0;
    assert_eq!(cell_f64(&state, x, 0), 1., "first line kept");
    assert_eq!(cell_f64(&state, x, 1), 2.);
}

/// The place of the drop sets the destination; the `.stf` extension makes a file
/// a project to open, not data to import.
#[test]
fn file_drop_plan_picks_destination_from_the_drop_target_and_splits_projects() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, state) = start_demo(dir.path());
    let book = book_named(state.tree(), "Book1");
    let paths = vec![
        std::path::PathBuf::from("a.csv"),
        std::path::PathBuf::from("projet.STF"),
        std::path::PathBuf::from("b.txt"),
    ];

    let on_book = file_drop_plan(&paths, Some(book));
    assert_eq!(on_book.destination, ImportDestination::ActiveBook(book));
    assert_eq!(
        on_book.projects,
        vec![std::path::PathBuf::from("projet.STF")]
    );
    assert_eq!(on_book.data.len(), 2);

    let elsewhere = file_drop_plan(&paths, None);
    assert_eq!(elsewhere.destination, ImportDestination::NewBookForAll);
}

/// Drop on a workbook: one sheet per file, a single history entry,
/// and the message names the template used.
#[test]
fn dropping_files_on_a_book_imports_them_as_one_undo_entry() {
    let _recipes = isolate_recipe_store();
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let book = book_named(state.tree(), "Book1");
    let sheets_before = state.worksheets_of(book).len();
    let file_a = dir.path().join("depot_a.csv");
    let file_b = dir.path().join("depot_b.csv");
    std::fs::write(&file_a, "1,10\n2,20\n").unwrap();
    std::fs::write(&file_b, "3,30\n4,40\n").unwrap();

    state.dispatch(Action::DropFiles {
        paths: vec![file_a, file_b],
        target_book: Some(book),
    });
    wait_import(&mut state);

    assert_eq!(state.worksheets_of(book).len(), sheets_before + 2);
    assert!(state.import_state().is_none(), "no import window");
    let status = state.status().expect("status message");
    assert!(!status.is_error());
    assert_eq!(
        status.text,
        stokesforge_app_state::tr!("status-import-dropped", count = 2, models = "Untitled"),
        "{}",
        status.text
    );
    state.dispatch(Action::Undo);
    assert_eq!(state.worksheets_of(book).len(), sheets_before);
    assert!(!state.can_undo(), "a single Ctrl+Z for the whole drop");
}

/// Drop outside a workbook: a new workbook shared by the batch.
#[test]
fn dropping_files_elsewhere_creates_a_new_book() {
    let _recipes = isolate_recipe_store();
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let root = state.tree().root();
    let books_before = state.tree().node(root).unwrap().children().len();
    let file = dir.path().join("depot.csv");
    std::fs::write(&file, "1,10\n2,20\n").unwrap();

    state.dispatch(Action::DropFiles {
        paths: vec![file],
        target_book: None,
    });
    wait_import(&mut state);

    assert_eq!(
        state.tree().node(root).unwrap().children().len(),
        books_before + 1
    );
}

/// A `.stf` is never imported: it is queued to be opened in
/// a new window, without touching the current project.
#[test]
fn dropping_a_project_file_queues_it_instead_of_importing() {
    let _recipes = isolate_recipe_store();
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let revision = state.revision();
    let project = dir.path().join("autre.stf");
    std::fs::write(&project, b"not read here").unwrap();

    state.dispatch(Action::DropFiles {
        paths: vec![project.clone()],
        target_book: None,
    });

    assert_eq!(state.revision(), revision, "the current project is intact");
    assert_eq!(state.take_projects_to_open(), vec![project]);
    assert!(state.take_projects_to_open().is_empty(), "queue emptied");
}

/// A file that the recipe reads badly is not imported: the import window
/// opens, prefilled; a correct file of the same batch is imported.
#[test]
fn dropping_an_unreadable_file_opens_the_import_window_prefilled() {
    let _recipes = isolate_recipe_store();
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let book = book_named(state.tree(), "Book1");
    let sheets_before = state.worksheets_of(book).len();
    let bad = dir.path().join("binaire.dat");
    std::fs::write(&bad, [0u8, 159, 146, 150, 0, 0]).unwrap();
    let good = dir.path().join("bon.csv");
    std::fs::write(&good, "1,10\n2,20\n").unwrap();

    state.dispatch(Action::DropFiles {
        paths: vec![bad.clone(), good],
        target_book: Some(book),
    });
    wait_import(&mut state);

    assert_eq!(state.worksheets_of(book).len(), sheets_before + 1);
    let import = state.import_state().expect("import window open");
    assert_eq!(import.paths, vec![bad]);
    assert_eq!(import.destination, ImportDestination::ActiveBook(book));
}

// -- Creating a workbook or a sheet: X and Y columns by default --

/// A new workbook is born with an X column and a Y column, and a single
/// Ctrl+Z removes everything.
#[test]
fn a_new_book_starts_with_an_x_and_a_y_column() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let root = state.tree().root();
    let before = state.tree().node(root).unwrap().children().len();

    state.dispatch(Action::AddWorkbook);

    let book = *state.tree().node(root).unwrap().children().last().unwrap();
    assert_eq!(
        state.tree().node(root).unwrap().children().len(),
        before + 1
    );
    let sheet = state.worksheets_of(book)[0].0;
    let columns = state.visible_columns(sheet);
    assert_eq!(columns.len(), 2);
    assert_eq!(state.column_meta(columns[0].0).role, ColumnRole::X);
    assert_eq!(state.column_meta(columns[1].0).role, ColumnRole::Y);

    state.dispatch(Action::Undo);
    assert_eq!(state.tree().node(root).unwrap().children().len(), before);
}

/// A sheet added to a workbook also gets X and Y.
#[test]
fn a_new_sheet_starts_with_an_x_and_a_y_column() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let book = book_named(state.tree(), "Book1");
    let before = state.worksheets_of(book).len();

    state.dispatch(Action::AddWorksheet { book });

    let sheets = state.worksheets_of(book);
    assert_eq!(sheets.len(), before + 1);
    let columns = state.visible_columns(sheets.last().unwrap().0);
    assert_eq!(columns.len(), 2);
    assert_eq!(state.column_meta(columns[0].0).role, ColumnRole::X);
    assert_eq!(state.column_meta(columns[1].0).role, ColumnRole::Y);

    state.dispatch(Action::Undo);
    assert_eq!(state.worksheets_of(book).len(), before);
}

// -- File metadata per column (issue #5) --

/// The import keeps the file header, key / value, on each column; the
/// viewing window opens and closes without touching the history;
/// a single Ctrl+Z removes the import and its metadata.
#[test]
fn import_keeps_file_metadata_on_every_column() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let book = book_named(state.tree(), "Book1");
    let file = dir.path().join("jasco.csv");
    std::fs::write(
        &file,
        "TITLE;S13\n[Measurement Information]\nXUNITS;NANOMETERS\nXYDATA\n800;1\n799;2\n",
    )
    .unwrap();
    let mut recipe = ImportRecipe::new("Jasco");
    recipe.separator = Separator::Semicolon;
    recipe.header = HeaderSkip::UntilMarker("XYDATA".into());

    state.dispatch(Action::ImportFiles {
        destination: ImportDestination::ActiveBook(book),
        recipe,
        paths: vec![file],
    });

    let sheet = sheet_named(state.tree(), "jasco");
    let columns = state.visible_columns(sheet);
    assert_eq!(columns.len(), 2);
    for (column, ..) in &columns {
        let lines = state.column_meta(*column).file_metadata;
        let keys: Vec<&str> = lines.iter().map(|l| l.key.as_str()).collect();
        assert_eq!(
            keys,
            vec!["TITLE", "[Measurement Information]", "XUNITS", ""]
        );
        assert_eq!(lines[3].value, "XYDATA", "line without a key kept as is");
        assert_eq!(lines[2].value, "NANOMETERS");
        assert!(lines[1].is_section());
    }
    assert_eq!(
        state.file_metadata_summary(columns[0].0),
        stokesforge_app_state::tr!("workbook-metadata-summary", count = 3)
    );

    let revision = state.revision();
    state.dispatch(Action::ViewColumnMetadata {
        column: columns[0].0,
    });
    assert_eq!(state.column_metadata_view(), Some(columns[0].0));
    state.dispatch(Action::CloseColumnMetadata);
    assert_eq!(state.column_metadata_view(), None);
    assert_eq!(
        state.revision(),
        revision,
        "viewing does not change the model"
    );

    state.dispatch(Action::Undo);
    assert!(state
        .worksheets_of(book)
        .iter()
        .all(|(id, _)| state.tree().node(*id).unwrap().name() != "jasco"));
}

/// A column typed by hand has no metadata: empty cell.
#[test]
fn a_hand_made_column_has_no_file_metadata() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    state.dispatch(Action::AddWorkbook);
    let root = state.tree().root();
    let book = *state.tree().node(root).unwrap().children().last().unwrap();
    let sheet = state.worksheets_of(book)[0].0;
    let column = state.visible_columns(sheet)[0].0;
    assert!(state.column_meta(column).file_metadata.is_empty());
    assert_eq!(state.file_metadata_summary(column), "");
}

// -- Step-by-step import wizard (issue #6) --

/// Saving the template sets the reference of the "modified" witness, even
/// after a rename. A new edit lights it up again.
#[test]
fn saving_an_import_template_updates_the_modified_witness() {
    let _recipes = isolate_recipe_store();
    let dir = tempfile::tempdir().unwrap();
    let (_session, mut state) = start_demo(dir.path());
    state.open_import(None::<NodeId>);
    {
        let import = state.import_state_mut().unwrap();
        import.recipe.skip_chars = 2;
        import.save_as_name = "My instrument".into();
    }
    assert!(state.import_recipe_modified());

    state.save_import_recipe();

    assert!(!state.status().unwrap().is_error());
    assert!(
        !state.import_recipe_modified(),
        "the template was just saved"
    );
    let saved = state.import_state().unwrap().recipe.clone();
    assert!(RecipeStore::load().recipes().contains(&saved));

    state.import_state_mut().unwrap().recipe.skip_chars = 3;
    assert!(state.import_recipe_modified());
    state.import_state_mut().unwrap().recipe.skip_chars = 2;
    assert!(
        !state.import_recipe_modified(),
        "back to the saved template"
    );
}

/// A write failure must not make one believe the settings are saved.
#[test]
fn a_save_failure_keeps_the_import_template_modified() {
    let _recipes = isolate_recipe_store();
    let dir = tempfile::tempdir().unwrap();
    let (_session, mut state) = start_demo(dir.path());
    state.open_import(None::<NodeId>);
    state.import_state_mut().unwrap().recipe.skip_chars = 2;
    state.save_import_recipe();
    assert!(!state.status().unwrap().is_error());
    state.import_state_mut().unwrap().recipe.skip_chars = 3;

    let file = stokesforge_import::config_dir::config_dir()
        .unwrap()
        .join("import_recipes.json");
    std::fs::remove_file(&file).unwrap();
    std::fs::create_dir(&file).unwrap();
    state.save_import_recipe();

    assert!(state.dialog_error().is_some(), "S-M7: in the wizard footer");
    assert!(state.import_recipe_modified());
    state.import_state_mut().unwrap().recipe.skip_chars = 2;
    assert!(
        !state.import_recipe_modified(),
        "the reference stays the last success"
    );
}

/// #267: an imported template whose saving fails stays "modified".
#[test]
fn a_save_failure_of_an_imported_template_keeps_it_modified() {
    let _recipes = isolate_recipe_store();
    let dir = tempfile::tempdir().unwrap();
    let (_session, mut state) = start_demo(dir.path());
    state.open_import(None::<NodeId>);
    let mut recipe = state.import_state_mut().unwrap().recipe.clone();
    recipe.name = "imported".to_string();
    recipe.skip_chars = 7;
    let json = dir.path().join("modele.json");
    std::fs::write(&json, recipe.to_json()).unwrap();

    let file = stokesforge_import::config_dir::config_dir()
        .unwrap()
        .join("import_recipes.json");
    let _ = std::fs::remove_file(&file);
    std::fs::create_dir_all(&file).unwrap();
    state.import_import_recipe(&json);

    assert!(state.dialog_error().is_some());
    assert!(
        state.import_recipe_modified(),
        "not written to disk: the reference stays the old one"
    );
}

fn jasco_like(dir: &std::path::Path) -> std::path::PathBuf {
    let file = dir.join("jasco.csv");
    std::fs::write(
        &file,
        "TITLE;S13\nXUNITS;NANOMETERS\nXYDATA\n800;1\n799;2\n798;3\n\nData points;3\n",
    )
    .unwrap();
    file
}

/// The steps follow one another in order, bounded at both ends.
#[test]
fn import_wizard_steps_go_forward_and_back_within_bounds() {
    use stokesforge_app_state::state::ImportStep;
    let _recipes = isolate_recipe_store();
    let dir = tempfile::tempdir().unwrap();
    let (_session, mut state) = start_demo(dir.path());
    state.open_import(None::<NodeId>);
    assert_eq!(state.import_state().unwrap().step, ImportStep::Files);
    state.import_step_back();
    assert_eq!(state.import_state().unwrap().step, ImportStep::Files);
    for _ in 0..10 {
        state.import_step_forward();
    }
    assert_eq!(state.import_state().unwrap().step, ImportStep::Destination);
    state.import_step_back();
    assert_eq!(state.import_state().unwrap().step, ImportStep::Metadata);
}

/// The file is read only once: after its deletion from disk, the preview
/// is still served from the cache (no re-read on every frame).
#[test]
fn import_preview_reads_the_file_only_once() {
    let _recipes = isolate_recipe_store();
    let dir = tempfile::tempdir().unwrap();
    let (_session, mut state) = start_demo(dir.path());
    let file = jasco_like(dir.path());
    state.open_import(None::<NodeId>);
    state.set_import_paths(vec![file.clone()]);
    assert!(matches!(state.import_line_preview(), Some(Ok(_))));
    std::fs::remove_file(&file).unwrap();
    assert!(
        matches!(state.import_line_preview(), Some(Ok(_))),
        "the preview comes from the cache, not from the disk"
    );
    assert_eq!(state.import_encoding(), Some("UTF-8"));
}

/// Several files: one is removed, the one to preview is chosen,
/// and each carries its own state (lines read or error) before the import.
#[test]
fn import_wizard_lists_files_with_their_status_and_previews_the_chosen_one() {
    let _recipes = isolate_recipe_store();
    let dir = tempfile::tempdir().unwrap();
    let (_session, mut state) = start_demo(dir.path());
    let good = jasco_like(dir.path());
    let missing = dir.path().join("absent.csv");
    state.open_import(None::<NodeId>);
    state.set_import_paths(vec![good.clone(), missing.clone()]);

    let statuses = state.import_file_statuses();
    assert!(statuses[0].1.is_ok());
    assert!(
        statuses[1].1.is_err(),
        "the error is known before importing"
    );

    state.set_import_preview_index(1);
    assert!(matches!(state.import_line_preview(), Some(Err(_))));
    state.remove_import_path(1);
    assert_eq!(state.import_state().unwrap().paths, vec![good]);
    assert!(matches!(state.import_line_preview(), Some(Ok(_))));
}

/// The two header settings are brought down to one when a template is
/// opened; deleting a template asks for a confirmation.
#[test]
fn import_wizard_unifies_header_and_confirms_recipe_deletion() {
    let _recipes = isolate_recipe_store();
    let dir = tempfile::tempdir().unwrap();
    let (_session, mut state) = start_demo(dir.path());
    state.open_import(None::<NodeId>);
    {
        let import = state.import_state_mut().unwrap();
        import.recipe.header = HeaderSkip::Lines(3);
        import.save_as_name = "Three lines".into();
    }
    state.save_import_recipe();
    state.open_import(None::<NodeId>);
    state.select_import_recipe("Three lines");
    let recipe = &state.import_state().unwrap().recipe;
    assert_eq!(recipe.header, HeaderSkip::None);
    assert_eq!(recipe.header_layout.main_header_lines, 3);
    assert!(!state.import_recipe_modified());

    state.import_state_mut().unwrap().recipe.skip_chars = 2;
    assert!(
        state.import_recipe_modified(),
        "modified without being saved"
    );

    state.request_delete_import_recipe("Three lines");
    state.cancel_delete_import_recipe();
    assert!(state
        .import_state()
        .unwrap()
        .store
        .recipes()
        .iter()
        .any(|r| r.name == "Three lines"));
    state.request_delete_import_recipe("Three lines");
    state.confirm_delete_import_recipe();
    assert!(state
        .import_state()
        .unwrap()
        .store
        .recipes()
        .iter()
        .all(|r| r.name != "Three lines"));
}

/// The right-click on the preview sets the header, the units row and the end
/// of the data.
#[test]
fn import_preview_line_actions_adjust_the_recipe() {
    use stokesforge_app_state::state::ImportLineAction;
    let _recipes = isolate_recipe_store();
    let dir = tempfile::tempdir().unwrap();
    let (_session, mut state) = start_demo(dir.path());
    let file = jasco_like(dir.path());
    state.open_import(None::<NodeId>);
    state.set_import_paths(vec![file]);
    {
        let recipe = &mut state.import_state_mut().unwrap().recipe;
        recipe.separator = Separator::Semicolon;
        recipe.header = HeaderSkip::None;
        recipe.header_layout = Default::default();
        // Automatic cut is no longer the default: this test exercises it.
        recipe.data_end = stokesforge_import::DataEnd::Auto;
    }

    // The data starts at line 4: three header lines.
    state.import_line_action(ImportLineAction::DataStartsHere, 4);
    assert_eq!(
        state
            .import_state()
            .unwrap()
            .recipe
            .header_layout
            .main_header_lines,
        3
    );
    assert_eq!(state.import_preview().unwrap().unwrap().row_count(), 3);

    // End of the data at line 6 (the last data is line 6).
    state.import_line_action(ImportLineAction::DataEndsHere, 6);
    assert_eq!(state.import_preview().unwrap().unwrap().row_count(), 2);

    state.import_line_action(ImportLineAction::NoAutoCut, 6);
    assert_eq!(
        state.import_state().unwrap().recipe.data_end,
        stokesforge_import::DataEnd::None
    );

    // Line 2 gives the units: it opens the subheader.
    state.import_line_action(ImportLineAction::UnitsLine, 2);
    let recipe = &state.import_state().unwrap().recipe;
    assert_eq!(recipe.header_layout.main_header_lines, 1);
    assert_eq!(recipe.header_layout.subheader_lines, 2);
    assert_eq!(
        recipe.subheader_roles.unit,
        Some(stokesforge_import::SubheaderLineRange::single(1))
    );
}

/// The "Columns" step: an Ignore role removes the column, Y error is
/// set on the chosen column.
#[test]
fn import_column_roles_from_the_wizard_are_applied() {
    let _recipes = isolate_recipe_store();
    let dir = tempfile::tempdir().unwrap();
    let (_session, mut state) = start_demo(dir.path());
    let book = book_named(state.tree(), "Book1");
    let file = dir.path().join("trois.csv");
    std::fs::write(&file, "1,10,0.1,7\n2,20,0.2,8\n").unwrap();
    let mut recipe = ImportRecipe::new("Colonnes");
    recipe.column_roles = vec![
        stokesforge_import::ColumnRoleChoice::Auto,
        stokesforge_import::ColumnRoleChoice::Y,
        stokesforge_import::ColumnRoleChoice::YError,
        stokesforge_import::ColumnRoleChoice::Ignore,
    ];

    state.dispatch(Action::ImportFiles {
        destination: ImportDestination::ActiveBook(book),
        recipe,
        paths: vec![file],
    });

    let sheet = sheet_named(state.tree(), "trois");
    let columns = state.visible_columns(sheet);
    assert_eq!(columns.len(), 3, "the fourth column is ignored");
    assert_eq!(state.column_meta(columns[0].0).role, ColumnRole::X);
    assert_eq!(state.column_meta(columns[1].0).role, ColumnRole::Y);
    assert_eq!(state.column_meta(columns[2].0).role, ColumnRole::YError);
}

/// A drop while the wizard is open adds the files to its list.
#[test]
fn dropping_files_while_the_wizard_is_open_adds_them_to_its_list() {
    let _recipes = isolate_recipe_store();
    let dir = tempfile::tempdir().unwrap();
    let (_session, mut state) = start_demo(dir.path());
    let file = jasco_like(dir.path());
    state.open_import(None::<NodeId>);
    state.dispatch(Action::DropFiles {
        paths: vec![file.clone()],
        target_book: None,
    });
    assert_eq!(state.import_state().unwrap().paths, vec![file.clone()]);
    state.dispatch(Action::DropFiles {
        paths: vec![file],
        target_book: None,
    });
    assert_eq!(
        state.import_state().unwrap().paths.len(),
        1,
        "without duplicates"
    );
}

// -- Undo state after closing a grouped operation (issue #22) --

/// The UI snapshot reflects the closed macro as soon as the action ends: same
/// `can_undo` and same label as the actor.
fn assert_undo_state_matches_actor(session: &Session, state: &AppState, label: HistoryLabel) {
    let actual = session.handle().snapshot().unwrap();
    assert_eq!(state.can_undo(), actual.history.can_undo);
    assert!(state.can_undo(), "{label:?} must be undoable");
    assert_eq!(actual.history.undo_label.as_ref(), Some(&label));
    assert_eq!(
        state.undo_label(),
        Some(undo_entry(&label)),
        "the menu label comes from the closed entry"
    );
}

#[test]
fn undo_state_is_fresh_right_after_a_grouped_paste() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (session, mut state) = start_demo(dir.path());
    let book = book_named(state.tree(), "Book1");
    let sheet = state.selected_sheet_in(book).unwrap();
    let columns = state.visible_columns(sheet);
    state.select_cell(sheet, columns[0].0, 0);
    // Fresh session: nothing undoable before the paste.
    while state.can_undo() {
        state.dispatch(Action::Undo);
    }
    state.dispatch(Action::Paste {
        sheet,
        text: "10\t20".into(),
    });
    assert_undo_state_matches_actor(&session, &state, HistoryLabel::Macro(MacroKind::PasteCells));
}

#[test]
fn undo_state_is_fresh_right_after_a_grouped_clear() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (session, mut state) = start_demo(dir.path());
    let book = book_named(state.tree(), "Book1");
    let sheet = state.selected_sheet_in(book).unwrap();
    let columns = state.visible_columns(sheet);
    state.select_cell(sheet, columns[0].0, 0);
    state.extend_selection(sheet, columns[1].0, 0);
    while state.can_undo() {
        state.dispatch(Action::Undo);
    }
    state.dispatch(Action::ClearSelectedCells { sheet });
    assert_undo_state_matches_actor(
        &session,
        &state,
        HistoryLabel::Macro(MacroKind::ClearSelection),
    );
}

// -- Undoing a graph's properties does not undo a detached input (#21) --

/// While the graph properties window holds a macro, a detached view
/// (other sender identity) cannot slip a write into it:
/// it is refused, and undoing the properties does not touch the workbook.
#[test]
fn cancelling_plot_settings_does_not_undo_a_detached_entry() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut main) = start_demo(dir.path());
    let graph = plotted_graph(&mut main);
    let book = book_named(main.tree(), "Book1");
    let sheet = main.selected_sheet_in(book).unwrap();
    let column = main.visible_columns(sheet)[0].0;
    let before = cell_f64(&main, column, 0);

    let mut detached = main.fork_for_detached_view().expect("detached view");
    main.dispatch(Action::OpenPlotSettings {
        graph,
        tab: PlotSettingsTab::Graph,
        axis: None,
    });
    detached.dispatch(Action::CommitCell {
        column,
        row: 0,
        text: "99".into(),
    });
    assert!(
        detached.status().is_some_and(|status| status.is_error()),
        "the foreign input is refused during the dialog"
    );
    main.cancel_plot_settings();
    detached.refresh();
    assert_eq!(cell_f64(&detached, column, 0), before);

    // The dialog closed, the same input goes through and stays.
    detached.dispatch(Action::CommitCell {
        column,
        row: 0,
        text: "99".into(),
    });
    main.refresh();
    assert_eq!(cell_f64(&main, column, 0), 99.0);
}

// -- Heavy recalculation: indicator and Stop (issue #239) --

/// A project whose first full recalculation lasts long enough to exceed
/// the command wait: a big source column and many formula columns
/// that read it.
fn heavy_formula_project() -> Project {
    let mut project = Project::new("Lourd").expect("project");
    let root = project.root();
    let book = project.add_workbook(root, Some("Book1")).expect("workbook");
    let sheet = project
        .tree()
        .child_by_name(book, "Sheet1")
        .expect("read")
        .expect("Sheet1");
    let values: Vec<f64> = (0..200_000).map(f64::from).collect();
    project
        .add_column(sheet, None, Some("A"), Column::from_f64(&values))
        .expect("A");
    let column_a = project.tree().children(sheet).expect("sheet")[0];
    for index in 0..150 {
        let column = project
            .add_column(
                sheet,
                None,
                Some(&format!("F{index}")),
                Column::new(ColumnType::Numeric),
            )
            .expect("column");
        // Formula by identifier (#321): it reads column A.
        let formula = ColumnFormula::from_parts(vec![
            FormulaPart::Column(column_a),
            FormulaPart::Text("*2+".into()),
            FormulaPart::Column(column_a),
            FormulaPart::Text("*3".into()),
        ]);
        project
            .set_column_meta(
                column,
                ColumnMeta {
                    formula: Some(formula),
                    ..Default::default()
                },
            )
            .expect("formula");
    }
    project
}

/// A command that starts a long recalculation hands control back: the indicator
/// can be read, and Stop undoes the command like Ctrl+Z (redoable), with its message.
/// Without Stop (witness), the same command stays applied.
#[test]
fn stop_during_a_heavy_recalculation_undoes_the_command() {
    let dir = tempfile::tempdir().unwrap();
    let (_session, mut state) = start(heavy_formula_project(), &dir.path().join("lourd.stf"));
    let folders = state.tree().len();
    state.dispatch(Action::AddGraph);
    let elapsed = state
        .recalc_work()
        .expect("the full recalculation outlasts the command wait");
    assert!(elapsed >= std::time::Duration::from_millis(200));
    assert_eq!(state.tree().len(), folders, "snapshot not refreshed");

    state.dispatch(Action::StopRecalc);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(120);
    while state.recalc_work().is_some() {
        assert!(
            std::time::Instant::now() < deadline,
            "recalculation too long"
        );
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    state.poll_recalc();
    assert_eq!(state.tree().len(), folders, "the graph disappeared");
    assert!(state.snapshot().history.can_redo, "as after Ctrl+Z");
    assert!(!state.snapshot().history.can_undo);
    let status = state.status().expect("message");
    assert!(!status.is_error());
    assert_eq!(
        status.text,
        stokesforge_app_state::tr!("status-recalc-stopped")
    );
}

#[test]
fn without_stop_the_heavy_recalculation_keeps_the_command() {
    let dir = tempfile::tempdir().unwrap();
    let (_session, mut state) = start(heavy_formula_project(), &dir.path().join("lourd.stf"));
    let folders = state.tree().len();
    state.dispatch(Action::AddGraph);
    assert!(state.recalc_work().is_some());
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(120);
    while state.recalc_work().is_some() {
        assert!(
            std::time::Instant::now() < deadline,
            "recalculation too long"
        );
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    state.poll_recalc();
    assert_eq!(state.tree().len(), folders + 1, "the graph stays");
    assert!(state.snapshot().history.can_undo);
}

/// The status bar messages, from the oldest to the most recent. The heavy
/// project weighs about 240 MB once written: if the temporary folder is full
/// (`Disk quota exceeded`), the write fails and only this message says why
/// (#342). To be attached to the assertions that depend on a write.
fn status_messages(state: &AppState) -> String {
    state
        .status_history()
        .map(|m| m.text.clone())
        .collect::<Vec<_>>()
        .join(" | ")
}

/// Waits for the current recalculation to end, then lets the state pick it up.
fn finish_recalculation(state: &mut AppState) {
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

/// Saved project, then a command whose recalculation exceeds the wait:
/// the snapshot keeps the old revision, but the command is in
/// the actor. Closing, creating a project or opening another one must raise
/// the "save?" question, otherwise the change is lost without a word.
#[test]
fn closing_during_a_heavy_recalculation_still_asks_to_save() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("lourd.stf");
    let (_session, mut state) = start(heavy_formula_project(), &file);
    state.dispatch(Action::Save);
    let clean_title = state.window_title();
    state.dispatch(Action::Quit {
        pending_edits: false,
    });
    assert!(state.take_quit_ready(), "saved: direct Quit");

    state.dispatch(Action::AddGraph);
    assert!(state.recalc_work().is_some(), "recalculation still running");
    assert_ne!(state.window_title(), clean_title, "title: modified");
    state.dispatch(Action::Quit {
        pending_edits: false,
    });
    assert!(state.unsaved_pending().is_some(), "the question is asked");
    assert!(!state.take_quit_ready());
    state.dispatch(Action::ConfirmUnsaved(UnsavedChoice::Cancel));

    state.dispatch(Action::NewProject {
        pending_edits: false,
    });
    assert!(state.unsaved_pending().is_some(), "New asks too");
    state.dispatch(Action::ConfirmUnsaved(UnsavedChoice::Cancel));
    finish_recalculation(&mut state);
}

/// Inverse witness: saving during the recalculation makes the project "clean" again
/// — the applied revision must not stay above the saved mark.
#[test]
fn saving_during_a_heavy_recalculation_clears_the_unsaved_mark() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("lourd.stf");
    let (_session, mut state) = start(heavy_formula_project(), &file);
    state.dispatch(Action::Save);
    state.dispatch(Action::AddGraph);
    assert!(state.recalc_work().is_some());
    state.dispatch(Action::Save);
    finish_recalculation(&mut state);
    state.dispatch(Action::Quit {
        pending_edits: false,
    });
    assert!(
        state.unsaved_pending().is_none(),
        "nothing left to lose; messages: {}",
        status_messages(&state)
    );
    assert!(state.take_quit_ready());
}

// -- Death of the session actor (#310) --

/// The actor dies: a single blocking alert, a readable rescue copy
/// of the last displayed state, never written to the user's file.
#[test]
fn a_dead_session_raises_one_blocking_alert_with_a_recovery_copy() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("travail.stf");
    let (session, mut state) = start(heavy_formula_project(), &file);
    state.dispatch(Action::AddGraph);
    finish_recalculation(&mut state);
    let tree = state.tree().clone();
    let recovery = dir.path().join("secours");
    state.set_recovery_dir(Some(recovery.clone()));
    assert!(!state.session_lost());
    assert!(state.alert().is_none());

    session.shutdown().unwrap();
    state.poll_recalc();

    assert!(state.session_lost());
    let alert = state.alert().expect("blocking alert");
    assert_eq!(
        alert.kind,
        stokesforge_app_state::state::AlertKind::SessionLost
    );
    // Only the `.stf` count: the copy also has its record (#317).
    let stf_files = |dir: &std::path::Path| -> Vec<std::path::PathBuf> {
        std::fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().path())
            .filter(|p| p.extension().is_some_and(|x| x == "stf"))
            .collect()
    };
    let copies = stf_files(&recovery);
    assert_eq!(
        copies.len(),
        1,
        "a rescue copy; alert: {}; messages: {}",
        alert.message,
        status_messages(&state)
    );
    assert!(alert.message.contains(&copies[0].display().to_string()));
    let back = stokesforge_persistence::load(&copies[0]).expect("the copy reads back");
    assert_eq!(back.tree(), &tree, "the last displayed state");
    assert!(!file.exists(), "the user's file is not touched");

    // Only once: neither a second copy nor a second alert.
    state.dispatch(Action::DismissAlert);
    assert!(state.take_quit_ready(), "Close closes the application");
    state.poll_recalc();
    state.refresh();
    assert!(state.alert().is_none());
    assert_eq!(stf_files(&recovery).len(), 1);
}

/// Dead session: closing the window does not open "save?" (there is
/// nothing left to save) and with no rescue folder the message stays empty.
#[test]
fn quitting_a_dead_session_asks_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let (session, mut state) = start(heavy_formula_project(), &dir.path().join("a.stf"));
    state.set_recovery_dir(None);
    state.dispatch(Action::AddGraph);
    finish_recalculation(&mut state);
    session.shutdown().unwrap();
    state.poll_recalc();
    assert!(state.session_lost());
    assert_eq!(state.alert().unwrap().message, "");

    state.dispatch(Action::Quit {
        pending_edits: false,
    });
    assert!(state.unsaved_pending().is_none());
    assert!(state.take_quit_ready());
}

// -- Background import, reopening the last import, recent files (issue #6) --

/// The analysis runs on a background thread: `start_import` hands control back,
/// progress can be read, and `poll_import` creates the sheets once ready —
/// always in a single history entry. A second launch is refused
/// while an import is running.
#[test]
fn a_background_import_reports_progress_then_applies_in_one_entry() {
    let _recipes = isolate_recipe_store();
    let dir = tempfile::tempdir().unwrap();
    let (_session, mut state) = start_demo(dir.path());
    let book = book_named(state.tree(), "Book1");
    let sheets_before = state.worksheets_of(book).len();
    let files: Vec<std::path::PathBuf> = (0..3)
        .map(|i| {
            let file = dir.path().join(format!("fond{i}.csv"));
            std::fs::write(&file, "1,10\n2,20\n").unwrap();
            file
        })
        .collect();

    state.dispatch(Action::StartImport {
        destination: ImportDestination::ActiveBook(book),
        recipe: ImportRecipe::new("Fond"),
        paths: files.clone(),
    });
    let (_, total) = state.import_progress().expect("an import is running");
    assert_eq!(total, 3);
    state.dispatch(Action::StartImport {
        destination: ImportDestination::ActiveBook(book),
        recipe: ImportRecipe::new("Fond"),
        paths: files.clone(),
    });
    assert!(
        state.status().is_some_and(|status| status.is_error()),
        "a second import is refused"
    );

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while !state.poll_import() {
        assert!(std::time::Instant::now() < deadline, "import too long");
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert!(state.import_progress().is_none());
    assert_eq!(state.worksheets_of(book).len(), sheets_before + 3);
    assert_eq!(state.take_imported_paths().len(), 3);
    assert!(state.take_imported_paths().is_empty(), "queue emptied");
    state.dispatch(Action::Undo);
    assert_eq!(state.worksheets_of(book).len(), sheets_before);
    assert!(!state.can_undo(), "a single entry for the whole batch");
}

/// Waits for the background import to end (read, analysis, then application by
/// `poll_import`), without ever waiting indefinitely.
fn wait_import(state: &mut AppState) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    while state.import_progress().is_some() {
        state.poll_import();
        assert!(std::time::Instant::now() < deadline, "import too long");
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
}

/// A drop of a big file hands control back right away (#266): reading and
/// analysis happen on the background thread, nothing is created yet.
#[test]
fn dropping_a_big_csv_returns_immediately_and_applies_later() {
    let _recipes = isolate_recipe_store();
    let dir = tempfile::tempdir().unwrap();
    let (_session, mut state) = start_demo(dir.path());
    let book = book_named(state.tree(), "Book1");
    let sheets_before = state.worksheets_of(book).len();
    let file = dir.path().join("gros.csv");
    let mut text = String::new();
    for i in 0..100_000 {
        text.push_str(&format!("{i},{},{},{}\n", i * 2, i * 3, i * 4));
    }
    std::fs::write(&file, text).unwrap();

    state.dispatch(Action::DropFiles {
        paths: vec![file],
        target_book: Some(book),
    });

    assert!(
        state.import_work().is_some(),
        "work in progress (S-P1/S-P2)"
    );
    assert_eq!(
        state.worksheets_of(book).len(),
        sheets_before,
        "nothing yet"
    );
    wait_import(&mut state);
    assert_eq!(state.worksheets_of(book).len(), sheets_before + 1);
}

/// An unreadable file does not block the good one: the wizard only opens when
/// the thread returns, with the unreadable file alone.
#[test]
fn a_background_drop_opens_the_wizard_only_when_the_thread_answers() {
    let _recipes = isolate_recipe_store();
    let dir = tempfile::tempdir().unwrap();
    let (_session, mut state) = start_demo(dir.path());
    let book = book_named(state.tree(), "Book1");
    let bad = dir.path().join("binaire.dat");
    std::fs::write(&bad, [0u8, 159, 146, 150, 0, 0]).unwrap();
    let good = dir.path().join("bon.csv");
    std::fs::write(&good, "1,10\n2,20\n").unwrap();
    state.dispatch(Action::DropFiles {
        paths: vec![bad.clone(), good],
        target_book: Some(book),
    });
    assert!(state.import_state().is_none(), "wizard closed meanwhile");
    wait_import(&mut state);
    assert_eq!(state.import_state().expect("wizard").paths, vec![bad]);
}

/// All unreadable: the wizard opens, no empty macro in the history.
#[test]
fn a_drop_of_only_unreadable_files_opens_the_wizard_without_history() {
    let _recipes = isolate_recipe_store();
    let dir = tempfile::tempdir().unwrap();
    let (_session, mut state) = start_demo(dir.path());
    let bad = dir.path().join("binaire.dat");
    std::fs::write(&bad, [0u8, 159, 146, 150, 0, 0]).unwrap();
    state.dispatch(Action::DropFiles {
        paths: vec![bad.clone()],
        target_book: None,
    });
    wait_import(&mut state);
    assert_eq!(state.import_state().expect("wizard").paths, vec![bad]);
    assert!(!state.can_undo(), "no history entry");
    assert_ne!(
        state.status().map(|status| status.text.clone()),
        Some(stokesforge_app_state::tr!(
            "status-import-dropped",
            count = 0,
            models = ""
        )),
        "no import applied, no drop message"
    );
}

/// Stop during a drop: nothing is applied, the wizard does not open.
#[test]
fn stopping_a_background_drop_applies_nothing() {
    let _recipes = isolate_recipe_store();
    let dir = tempfile::tempdir().unwrap();
    let (_session, mut state) = start_demo(dir.path());
    let book = book_named(state.tree(), "Book1");
    let sheets_before = state.worksheets_of(book).len();
    let bad = dir.path().join("binaire.dat");
    std::fs::write(&bad, [0u8, 159, 146, 150, 0, 0]).unwrap();
    let good = dir.path().join("bon.csv");
    std::fs::write(&good, "1,10\n2,20\n").unwrap();
    state.dispatch(Action::DropFiles {
        paths: vec![bad, good],
        target_book: Some(book),
    });
    state.dispatch(Action::StopImport);
    std::thread::sleep(std::time::Duration::from_millis(200));
    assert!(!state.poll_import());
    assert_eq!(state.worksheets_of(book).len(), sheets_before);
    assert!(state.import_state().is_none());
    assert!(!state.can_undo());
}

/// The "models used" message belongs to the drop: a menu import
/// does not show it.
#[test]
fn a_menu_import_does_not_report_a_drop() {
    let _recipes = isolate_recipe_store();
    let dir = tempfile::tempdir().unwrap();
    let (_session, mut state) = start_demo(dir.path());
    let book = book_named(state.tree(), "Book1");
    let file = dir.path().join("menu.csv");
    std::fs::write(&file, "1,10\n2,20\n").unwrap();
    state.dispatch(Action::StartImport {
        destination: ImportDestination::ActiveBook(book),
        recipe: ImportRecipe::new("Menu"),
        paths: vec![file],
    });
    wait_import(&mut state);
    let status = state.status().unwrap();
    assert!(!status.is_error());
    assert_ne!(
        status.text,
        stokesforge_app_state::tr!("status-import-dropped", count = 1, models = ""),
        "no drop message"
    );
}

/// A drop during a running import is refused.
#[test]
fn dropping_files_during_a_running_import_is_refused() {
    let _recipes = isolate_recipe_store();
    let dir = tempfile::tempdir().unwrap();
    let (_session, mut state) = start_demo(dir.path());
    let book = book_named(state.tree(), "Book1");
    let a = dir.path().join("a.csv");
    std::fs::write(&a, "1,10\n2,20\n").unwrap();
    state.dispatch(Action::DropFiles {
        paths: vec![a.clone()],
        target_book: Some(book),
    });
    assert!(state.import_work().is_some());
    state.dispatch(Action::DropFiles {
        paths: vec![a],
        target_book: Some(book),
    });
    let status = state.status().unwrap();
    assert!(status.is_error());
    assert_eq!(
        status.text,
        stokesforge_app_state::tr!("error-import-running")
    );
    wait_import(&mut state);
}

/// Graph properties open when the thread answers: the application is
/// deferred, the graph's macro stays intact, then the import is applied as a
/// separate entry.
#[test]
fn a_background_drop_waits_for_the_plot_settings_to_close() {
    let _recipes = isolate_recipe_store();
    let dir = tempfile::tempdir().unwrap();
    let (session, mut state) = start_demo(dir.path());
    let book = book_named(state.tree(), "Book1");
    let graph = plotted_graph(&mut state);
    let sheets_before = state.worksheets_of(book).len();
    let file = dir.path().join("depot.csv");
    std::fs::write(&file, "1,10\n2,20\n").unwrap();
    state.dispatch(Action::DropFiles {
        paths: vec![file],
        target_book: Some(book),
    });
    state.dispatch(Action::OpenPlotSettings {
        graph,
        tab: PlotSettingsTab::Graph,
        axis: None,
    });
    let before = session.handle().snapshot().unwrap();
    assert!(before.history.in_macro, "the dialog opens its macro");
    std::thread::sleep(std::time::Duration::from_millis(300));
    assert!(!state.poll_import(), "deferred");
    let during = session.handle().snapshot().unwrap();
    assert_eq!(*during.tree, *before.tree);
    assert!(during.history.in_macro);
    assert!(state.import_progress().is_some(), "work kept");
    state.cancel_plot_settings();
    wait_import(&mut state);
    assert_eq!(state.worksheets_of(book).len(), sheets_before + 1);
}

/// Stop (#232, S-P2): the background import stops, nothing is created, nor
/// history, and a new import can start right away.
#[test]
fn stopping_a_background_import_creates_nothing() {
    let _recipes = isolate_recipe_store();
    let dir = tempfile::tempdir().unwrap();
    let (_session, mut state) = start_demo(dir.path());
    let book = book_named(state.tree(), "Book1");
    let sheets_before = state.worksheets_of(book).len();
    let files: Vec<std::path::PathBuf> = (0..3)
        .map(|i| {
            let file = dir.path().join(format!("stop{i}.csv"));
            std::fs::write(&file, "1,10\n2,20\n").unwrap();
            file
        })
        .collect();
    let start = |state: &mut AppState| {
        state.dispatch(Action::StartImport {
            destination: ImportDestination::ActiveBook(book),
            recipe: ImportRecipe::new("Stop"),
            paths: files.clone(),
        });
    };

    start(&mut state);
    assert!(state.import_work().is_some(), "an import is in progress");
    state.dispatch(Action::StopImport);
    assert!(state.import_work().is_none() && state.import_progress().is_none());
    assert!(!state.poll_import(), "nothing to apply");
    assert_eq!(state.worksheets_of(book).len(), sheets_before);
    assert!(state.take_imported_paths().is_empty());
    assert!(!state.can_undo(), "no history entry");
    assert!(
        state.status().is_some_and(|status| !status.is_error()),
        "an intended stop is not an error"
    );

    // Witness: an import started after Stop goes all the way.
    start(&mut state);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while !state.poll_import() {
        assert!(std::time::Instant::now() < deadline, "import too long");
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert_eq!(state.worksheets_of(book).len(), sheets_before + 3);
}

/// "Adjust last import" undoes the import if it is still at the top of the
/// history, then reopens the wizard on its files and its recipe.
#[test]
fn reopening_the_last_import_undoes_it_and_prefills_the_wizard() {
    let _recipes = isolate_recipe_store();
    let dir = tempfile::tempdir().unwrap();
    let (_session, mut state) = start_demo(dir.path());
    let book = book_named(state.tree(), "Book1");
    assert!(!state.can_reopen_last_import());
    let file = dir.path().join("reprise.csv");
    std::fs::write(&file, "1;10\n2;20\n").unwrap();
    let mut recipe = ImportRecipe::new("Point-virgule");
    recipe.separator = Separator::Semicolon;
    let sheets_before = state.worksheets_of(book).len();
    state.dispatch(Action::ImportFiles {
        destination: ImportDestination::ActiveBook(book),
        recipe: recipe.clone(),
        paths: vec![file.clone()],
    });
    assert_eq!(state.worksheets_of(book).len(), sheets_before + 1);
    assert!(state.can_reopen_last_import());

    state.dispatch(Action::ReopenLastImport);

    assert_eq!(state.worksheets_of(book).len(), sheets_before, "undone");
    let import = state.import_state().expect("wizard reopened");
    assert_eq!(import.paths, vec![file]);
    assert_eq!(import.recipe.separator, Separator::Semicolon);
    assert_eq!(import.destination, ImportDestination::ActiveBook(book));
}

/// If something else was done since, the old import is not undone: one never
/// undoes foreign work.
#[test]
fn reopening_the_last_import_leaves_later_work_alone() {
    let _recipes = isolate_recipe_store();
    let dir = tempfile::tempdir().unwrap();
    let (_session, mut state) = start_demo(dir.path());
    let book = book_named(state.tree(), "Book1");
    let file = dir.path().join("reprise.csv");
    std::fs::write(&file, "1,10\n2,20\n").unwrap();
    state.dispatch(Action::ImportFiles {
        destination: ImportDestination::ActiveBook(book),
        recipe: ImportRecipe::new("Untitled"),
        paths: vec![file],
    });
    state.dispatch(Action::AddWorkbook);
    let root = state.tree().root();
    let books = state.tree().node(root).unwrap().children().len();

    state.dispatch(Action::ReopenLastImport);

    assert_eq!(state.tree().node(root).unwrap().children().len(), books);
    assert!(
        state.worksheets_of(book).len() >= 2,
        "the import stays in place"
    );
    assert!(state.import_state().is_some());
}

// -- Test templates, automatic choice, sharing (issue #7) --

fn fixture_path(name: &str) -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../test/import")
        .join(name)
}

/// A dropped Jasco file is read with the saved template that recognizes it
/// (signature `ORIGIN;JASCO`), with no template ever saved — the
/// numbers with a comma are read (decimal detected).
#[test]
fn dropping_a_jasco_file_picks_the_saved_model_automatically() {
    let _recipes = with_test_recipes();
    let dir = tempfile::tempdir().unwrap();
    let (_session, mut state) = start_demo(dir.path());
    let book = book_named(state.tree(), "Book1");
    // The recognized template takes precedence even over the last template used, whose
    // settings do not suit the Jasco structure at all.
    let initial = dir.path().join("initial.csv");
    std::fs::write(&initial, "Time,Signal\n1,10\n").unwrap();
    let mut previous_recipe = stokesforge_import::ImportRecipe::new("Last CSV");
    previous_recipe.has_column_header_row = true;
    state.dispatch(Action::ImportFiles {
        destination: ImportDestination::ActiveBook(book),
        recipe: previous_recipe,
        paths: vec![initial],
    });
    let before = state.worksheets_of(book).len();

    state.dispatch(Action::DropFiles {
        paths: vec![fixture_path("jasco_uvvis_semicolon.csv")],
        target_book: Some(book),
    });
    wait_import(&mut state);

    assert_eq!(state.worksheets_of(book).len(), before + 1);
    assert!(state.import_state().is_none(), "read without wizard");
    let status = state.status().expect("message");
    assert!(status.text.contains("Jasco (semicolon)"), "{}", status.text);
    let sheet = sheet_named(state.tree(), "jasco_uvvis_semicolon");
    let columns = state.visible_columns(sheet);
    assert_eq!(cell_f64(&state, columns[0].0, 0), 800.0);
    assert_eq!(column_type(&state, columns[0].0), ColumnType::Numeric);
}

/// In the wizard, the first file added chooses the template from its
/// content as long as the current template has not been modified.
#[test]
fn the_wizard_chooses_the_model_from_the_first_file() {
    let _recipes = with_test_recipes();
    let dir = tempfile::tempdir().unwrap();
    let (_session, mut state) = start_demo(dir.path());
    state.open_import(None::<NodeId>);
    // The window starts on the first saved template; start from a blank one.
    assert_eq!(
        state.import_state().unwrap().recipe.name,
        "Jasco (semicolon)"
    );
    state.new_import_recipe();
    assert_eq!(
        state.import_state().unwrap().recipe.name,
        stokesforge_app_state::tr!("import-template-untitled")
    );

    state.add_import_paths(vec![fixture_path("jasco_fluo_tab.txt")]);

    assert_eq!(state.import_state().unwrap().recipe.name, "Jasco (tab)");
    assert!(!state.import_recipe_modified());
    // A template modified by hand is not overwritten by a second add.
    state.import_state_mut().unwrap().recipe.skip_chars = 1;
    state.add_import_paths(vec![fixture_path("rpe_nohead_tab.asc")]);
    assert_eq!(state.import_state().unwrap().recipe.name, "Jasco (tab)");
}

/// No template exists in the program (#378): with an empty store the window
/// lists none and starts on the permissive recipe, and a dropped Jasco file
/// is read with it.
#[test]
fn without_saved_templates_the_import_starts_untitled_and_reads_a_drop() {
    let _recipes = isolate_recipe_store();
    let dir = tempfile::tempdir().unwrap();
    let (_session, mut state) = start_demo(dir.path());
    let book = book_named(state.tree(), "Book1");
    state.open_import(None::<NodeId>);
    let import = state.import_state().unwrap();
    assert!(import.store.recipes().is_empty());
    assert_eq!(
        import.recipe.name,
        stokesforge_app_state::tr!("import-template-untitled")
    );
    state.cancel_import();

    let before = state.worksheets_of(book).len();
    state.dispatch(Action::DropFiles {
        paths: vec![fixture_path("jasco_uvvis_semicolon.csv")],
        target_book: Some(book),
    });
    wait_import(&mut state);
    assert_eq!(state.worksheets_of(book).len(), before + 1);
    let status = state.status().expect("message");
    assert!(!status.text.contains("Jasco"), "{}", status.text);
}

/// A template is exported to a file and re-imported (same name: it
/// replaces), and it is then saved for the user.
#[test]
fn a_model_can_be_exported_and_imported_from_a_file() {
    let _recipes = with_test_recipes();
    let dir = tempfile::tempdir().unwrap();
    let (_session, mut state) = start_demo(dir.path());
    state.open_import(None::<NodeId>);
    state.select_import_recipe("EPR (tab)");
    state.import_state_mut().unwrap().recipe.skip_chars = 4;
    let file = dir.path().join("rpe.json");
    state.export_import_recipe(&file);
    assert!(file.exists());

    state.open_import(None::<NodeId>);
    state.import_import_recipe(&file);

    let recipe = &state.import_state().unwrap().recipe;
    assert_eq!(recipe.name, "EPR (tab)");
    assert_eq!(recipe.skip_chars, 4);
    assert!(stokesforge_import::RecipeStore::load()
        .recipes()
        .iter()
        .any(|r| r.name == "EPR (tab)" && r.skip_chars == 4));

    // A file that is not a template: error message, nothing changes.
    let bad = dir.path().join("pas_un_modele.json");
    std::fs::write(&bad, "{ not json").unwrap();
    state.import_import_recipe(&bad);
    assert!(state.dialog_error().is_some(), "S-M7: in the wizard footer");
    assert_eq!(state.import_state().unwrap().recipe.skip_chars, 4);
}

/// Several templates in one drop: each file has its own, all in
/// a single history entry.
#[test]
fn dropping_files_of_different_formats_uses_one_model_each_in_one_entry() {
    let _recipes = with_test_recipes();
    let dir = tempfile::tempdir().unwrap();
    let (_session, mut state) = start_demo(dir.path());
    let book = book_named(state.tree(), "Book1");
    let before = state.worksheets_of(book).len();

    state.dispatch(Action::DropFiles {
        paths: vec![
            fixture_path("jasco_uvvis_semicolon.csv"),
            fixture_path("rpe_nohead_tab.asc"),
        ],
        target_book: Some(book),
    });
    wait_import(&mut state);

    assert_eq!(state.worksheets_of(book).len(), before + 2);
    let status = state.status().unwrap();
    assert!(status.text.contains("Jasco (semicolon)"));
    assert!(status.text.contains("EPR (tab)"));
    state.dispatch(Action::Undo);
    assert_eq!(state.worksheets_of(book).len(), before);
}

/// Date-time column and column forced to text at import (issue #7).
#[test]
fn import_builds_datetime_and_forced_text_columns() {
    let _recipes = isolate_recipe_store();
    let dir = tempfile::tempdir().unwrap();
    let (_session, mut state) = start_demo(dir.path());
    let book = book_named(state.tree(), "Book1");
    let file = dir.path().join("kinds.csv");
    std::fs::write(
        &file,
        "007;2000-01-01 00:00:00;1\n008;2000-01-02 00:00:00;2\n",
    )
    .unwrap();
    let mut recipe = ImportRecipe::new("Natures");
    recipe.separator = Separator::Semicolon;
    recipe.column_kinds = vec![
        stokesforge_import::ColumnKind::Text,
        stokesforge_import::ColumnKind::DateTime,
    ];

    state.dispatch(Action::ImportFiles {
        destination: ImportDestination::ActiveBook(book),
        recipe,
        paths: vec![file],
    });

    let sheet = sheet_named(state.tree(), "kinds");
    let columns = state.visible_columns(sheet);
    assert_eq!(column_type(&state, columns[0].0), ColumnType::Mixed);
    match cell(&state, columns[0].0, 0) {
        Cell::Text(Some(text)) => assert_eq!(text, "007"),
        other => panic!("expected the text 007, got {other:?}"),
    }
    assert_eq!(column_type(&state, columns[1].0), ColumnType::DateTime);
    assert_eq!(cell_f64(&state, columns[1].0, 0), 946_684_800.0);
    assert_eq!(column_type(&state, columns[2].0), ColumnType::Numeric);
}

// -- Import modes and distribution of several files (issue #7) --

fn write_csv(dir: &std::path::Path, name: &str, content: &str) -> std::path::PathBuf {
    let file = dir.join(name);
    std::fs::write(&file, content).unwrap();
    file
}

fn column_values(state: &AppState, sheet: NodeId, position: usize) -> Vec<f64> {
    let (column, ..) = state.visible_columns(sheet)[position].clone();
    (0..state.row_count(sheet))
        .map(|row| cell_f64(state, column, row))
        .collect()
}

fn import_with(
    state: &mut AppState,
    book: NodeId,
    mode: stokesforge_import::ImportMode,
    layout: stokesforge_import::FilesLayout,
    files: Vec<std::path::PathBuf>,
) {
    let mut recipe = ImportRecipe::new("Modes");
    recipe.import_mode = mode;
    recipe.files_layout = layout;
    state.dispatch(Action::ImportFiles {
        destination: ImportDestination::ActiveBook(book),
        recipe,
        paths: files,
    });
}

/// Replacing the active sheet empties its previous columns and keeps the sheet;
/// a single Ctrl+Z gives back the old content.
#[test]
fn import_mode_replace_sheet_swaps_the_content_in_one_entry() {
    use stokesforge_import::{FilesLayout, ImportMode};
    let _recipes = isolate_recipe_store();
    let dir = tempfile::tempdir().unwrap();
    let (_session, mut state) = start_demo(dir.path());
    let book = book_named(state.tree(), "Book1");
    let sheet = state.selected_sheet_in(book).unwrap();
    let sheets = state.worksheets_of(book).len();
    let before = state.visible_columns(sheet).len();
    assert!(before > 0);
    let file = write_csv(dir.path(), "neuf.csv", "1,10\n2,20\n3,30\n");

    import_with(
        &mut state,
        book,
        ImportMode::ReplaceSheet,
        FilesLayout::SheetPerFile,
        vec![file],
    );

    assert_eq!(state.worksheets_of(book).len(), sheets, "no new sheet");
    assert_eq!(state.visible_columns(sheet).len(), 2);
    assert_eq!(column_values(&state, sheet, 0), vec![1.0, 2.0, 3.0]);
    state.dispatch(Action::Undo);
    assert_eq!(
        state.visible_columns(sheet).len(),
        before,
        "the old content comes back"
    );
}

/// Add columns: after those of the active sheet.
#[test]
fn import_mode_add_columns_appends_to_the_active_sheet() {
    use stokesforge_import::{FilesLayout, ImportMode};
    let _recipes = isolate_recipe_store();
    let dir = tempfile::tempdir().unwrap();
    let (_session, mut state) = start_demo(dir.path());
    let book = book_named(state.tree(), "Book1");
    let sheet = state.selected_sheet_in(book).unwrap();
    let before = state.visible_columns(sheet).len();
    let file = write_csv(dir.path(), "plus.csv", "7,70\n8,80\n");

    import_with(
        &mut state,
        book,
        ImportMode::AddColumns,
        FilesLayout::SheetPerFile,
        vec![file],
    );

    assert_eq!(state.visible_columns(sheet).len(), before + 2);
    assert_eq!(column_values(&state, sheet, before)[..2], [7.0, 8.0]);
}

/// Add rows: column j of the file extends column j of the
/// sheet.
#[test]
fn import_mode_add_rows_extends_the_existing_columns() {
    use stokesforge_import::{FilesLayout, ImportMode};
    let _recipes = isolate_recipe_store();
    let dir = tempfile::tempdir().unwrap();
    let (_session, mut state) = start_demo(dir.path());
    let book = book_named(state.tree(), "Book1");
    let sheet = state.selected_sheet_in(book).unwrap();
    let rows_before = state.row_count(sheet);
    let columns_before = state.visible_columns(sheet).len();
    let first_before = column_values(&state, sheet, 0);
    let file = write_csv(dir.path(), "suite.csv", "100,200\n101,201\n");

    import_with(
        &mut state,
        book,
        ImportMode::AddRows,
        FilesLayout::SheetPerFile,
        vec![file],
    );

    assert_eq!(state.row_count(sheet), rows_before + 2);
    assert_eq!(
        state.visible_columns(sheet).len(),
        columns_before,
        "same columns"
    );
    let first = column_values(&state, sheet, 0);
    assert_eq!(first[..rows_before], first_before[..]);
    assert_eq!(first[rows_before..], [100.0, 101.0]);
    state.dispatch(Action::Undo);
    assert_eq!(state.row_count(sheet), rows_before);
}

/// Several files, a single sheet: side by side (X Y X Y), or with the X
/// of the first file shared.
#[test]
fn several_files_can_share_one_sheet_side_by_side_or_with_a_common_x() {
    use stokesforge_import::{FilesLayout, ImportMode};
    let _recipes = isolate_recipe_store();
    let dir = tempfile::tempdir().unwrap();
    let (_session, mut state) = start_demo(dir.path());
    let book = book_named(state.tree(), "Book1");
    let a = write_csv(dir.path(), "spec_a.csv", "1,10\n2,20\n");
    let b = write_csv(dir.path(), "spec_b.csv", "1,11\n2,21\n");
    let sheets = state.worksheets_of(book).len();

    import_with(
        &mut state,
        book,
        ImportMode::NewSheet,
        FilesLayout::SideBySide,
        vec![a.clone(), b.clone()],
    );
    assert_eq!(
        state.worksheets_of(book).len(),
        sheets + 1,
        "a single sheet"
    );
    let sheet = sheet_named(state.tree(), "spec_a");
    assert_eq!(state.visible_columns(sheet).len(), 4, "X Y X Y");
    assert_eq!(column_values(&state, sheet, 3), vec![11.0, 21.0]);
    state.dispatch(Action::Undo);
    assert_eq!(state.worksheets_of(book).len(), sheets);

    import_with(
        &mut state,
        book,
        ImportMode::NewSheet,
        FilesLayout::SharedX,
        vec![a, b],
    );
    let sheet = sheet_named(state.tree(), "spec_a");
    let columns = state.visible_columns(sheet);
    assert_eq!(columns.len(), 3, "one X, then one Y per file");
    assert_eq!(state.column_meta(columns[0].0).role, ColumnRole::X);
    assert_eq!(state.column_meta(columns[2].0).role, ColumnRole::Y);
    assert_eq!(column_values(&state, sheet, 1), vec![10.0, 20.0]);
    assert_eq!(column_values(&state, sheet, 2), vec![11.0, 21.0]);
    // The Ys carry the name of their file for the legend.
    assert_eq!(state.column_meta(columns[1].0).long_name, "spec_a");
    assert_eq!(state.column_meta(columns[2].0).long_name, "spec_b");
}

/// Re-import (issue #7): the imported columns are re-read from their file
/// with the memorized recipe, **in place** — same columns (so the graphs
/// that plot them stay valid), new values, row count followed —,
/// in a single undoable entry.
#[test]
fn refreshing_from_the_source_file_updates_the_columns_in_place() {
    let _recipes = isolate_recipe_store();
    let dir = tempfile::tempdir().unwrap();
    let (_session, mut state) = start_demo(dir.path());
    let book = book_named(state.tree(), "Book1");
    let file = write_csv(dir.path(), "mesure.csv", "1;10\n2;20\n");
    let mut recipe = ImportRecipe::new("Point-virgule");
    recipe.separator = Separator::Semicolon;
    state.dispatch(Action::ImportFiles {
        destination: ImportDestination::ActiveBook(book),
        recipe,
        paths: vec![file.clone()],
    });
    let sheet = sheet_named(state.tree(), "mesure");
    assert!(state.can_refresh_from_source(sheet));
    let ids_before: Vec<NodeId> = state.visible_columns(sheet).iter().map(|c| c.0).collect();

    // The file changes: more lines, other values.
    write_csv(dir.path(), "mesure.csv", "5;50\n6;60\n7;70\n");
    state.dispatch(Action::RefreshFromSource { sheet });

    let ids_after: Vec<NodeId> = state.visible_columns(sheet).iter().map(|c| c.0).collect();
    assert_eq!(ids_before, ids_after, "the columns keep their identity");
    assert_eq!(column_values(&state, sheet, 0), vec![5.0, 6.0, 7.0]);
    assert_eq!(column_values(&state, sheet, 1), vec![50.0, 60.0, 70.0]);
    assert!(state.status().is_some_and(|s| !s.is_error()));

    // A single Ctrl+Z goes back to the values from before the refresh.
    state.dispatch(Action::Undo);
    assert_eq!(column_values(&state, sheet, 0), vec![1.0, 2.0]);

    // A vanished file is reported without breaking anything.
    std::fs::remove_file(&file).unwrap();
    state.dispatch(Action::RefreshFromSource { sheet });
    assert!(state.status().is_some_and(|s| s.is_error()));
    assert_eq!(column_values(&state, sheet, 0), vec![1.0, 2.0]);
    // The demo sheet, typed by hand, has nothing to refresh.
    let demo_sheet = state.selected_sheet_in(book).unwrap();
    assert!(!state.can_refresh_from_source(demo_sheet));
}

/// A file with several blocks gives one sheet per block (named after the
/// file and the rank), or a common sheet depending on the chosen distribution;
/// the refresh finds the right block of each column (issue #7).
#[test]
fn a_multi_block_file_gives_one_sheet_per_block_and_refreshes_per_block() {
    use stokesforge_import::{BlockSplit, HeaderSkip};
    let _recipes = isolate_recipe_store();
    let dir = tempfile::tempdir().unwrap();
    let (_session, mut state) = start_demo(dir.path());
    let book = book_named(state.tree(), "Book1");
    let before = state.worksheets_of(book).len();
    let file = write_csv(
        dir.path(),
        "spectres.csv",
        "TITLE;a\nXYDATA\n1;10\n2;20\nTITLE;b\nXYDATA\n3;30\n4;40\n5;50\n",
    );
    let mut recipe = ImportRecipe::new("Blocs");
    recipe.separator = Separator::Semicolon;
    recipe.header = HeaderSkip::UntilMarker("XYDATA".into());
    recipe.block_split = BlockSplit::BeforeLine("TITLE".into());

    state.dispatch(Action::ImportFiles {
        destination: ImportDestination::ActiveBook(book),
        recipe,
        paths: vec![file.clone()],
    });

    assert_eq!(state.worksheets_of(book).len(), before + 2);
    let first = sheet_named(state.tree(), "spectres_1");
    let second = sheet_named(state.tree(), "spectres_2");
    assert_eq!(column_values(&state, first, 1), vec![10.0, 20.0]);
    assert_eq!(column_values(&state, second, 1), vec![30.0, 40.0, 50.0]);
    assert_eq!(
        state.status().unwrap().text,
        stokesforge_app_state::tr!("status-import-done", imported = 1, total = 1)
    );

    // The file changes: each block updates in its own sheet.
    write_csv(
        dir.path(),
        "spectres.csv",
        "TITLE;a\nXYDATA\n1;11\n2;22\nTITLE;b\nXYDATA\n3;33\n4;44\n5;55\n",
    );
    state.dispatch(Action::RefreshFromSource { sheet: first });
    state.dispatch(Action::RefreshFromSource { sheet: second });
    assert_eq!(column_values(&state, first, 1), vec![11.0, 22.0]);
    assert_eq!(column_values(&state, second, 1), vec![33.0, 44.0, 55.0]);

    // A single Ctrl+Z for the whole import.
    while state.can_undo() {
        state.dispatch(Action::Undo);
    }
    assert_eq!(state.worksheets_of(book).len(), before);
}

// -- Issue #34: export a graph --

/// A demo project with a graph titled in Typst.
fn demo_with_graph(dir: &Path) -> (Session, AppState, NodeId) {
    let mut project = demo_project();
    let root = project.root();
    let sheet = sheet_named(project.tree(), "Sheet1");
    let column = |name: &str| {
        project
            .tree()
            .child_by_name(sheet, name)
            .expect("Sheet1 readable")
            .expect("column")
    };
    let (x, y) = (column("Time"), column("Signal"));
    let mut panel = stokesforge_plot::PanelSpec::at(0, 0);
    panel
        .series
        .push(stokesforge_plot::SeriesSpec::new_xy(x, y));
    let spec = stokesforge_plot::PlotSpec::empty("Spectrum $Delta H$").with_panels(panel);
    let graph = project
        .add_graph(root, Some("Graph1"), spec)
        .expect("graph");
    let (session, state) = start(project, &dir.join("projet.stf"));
    (session, state, graph)
}

fn graph_spec(state: &AppState, graph: NodeId) -> stokesforge_plot::PlotSpec {
    match state.tree().get(graph).expect("graph").kind() {
        stokesforge_core::node::NodeKind::Graph { spec, .. } => spec.clone(),
        _ => panic!("not a graph"),
    }
}

/// The export writes what it is given — the spec framed by the view, so the
/// current zoom — without touching the model: neither the graph's `PlotSpec` nor the
/// revision moves. The witness is the export of the unframed spec, which
/// differs.
#[test]
fn exporting_a_zoomed_graph_does_not_touch_the_model() {
    let dir = tempfile::tempdir().expect("temporary folder");
    let (_session, mut state, graph) = demo_with_graph(dir.path());
    let before = (graph_spec(&state, graph), state.snapshot().revision);

    // What `GraphView::framed_spec` returns after a zoom: a framed clone.
    let mut zoomed = before.0.clone();
    if let Some(panel) = zoomed.panels.get_mut(0) {
        panel.bottom.auto_range = false;
        panel.bottom.range_min = Some(0.5);
        panel.bottom.range_max = Some(1.0);
    }
    let exported = |state: &mut AppState, name: &str, spec: &stokesforge_plot::PlotSpec| {
        let path = dir.path().join(name);
        state.dispatch(Action::ExportGraph {
            path: path.clone(),
            spec: Box::new(spec.clone()),
        });
        let status = state.status().expect("a message");
        assert!(!status.is_error(), "{}", status.text);
        std::fs::read(path).expect("exported file")
    };
    let zoomed_svg = exported(&mut state, "zoome.svg", &zoomed);
    let whole_svg = exported(&mut state, "entier.svg", &before.0);
    assert_ne!(zoomed_svg, whole_svg, "the zoomed framing was not exported");
    assert!(String::from_utf8_lossy(&zoomed_svg).contains("<svg"));

    assert_eq!(graph_spec(&state, graph), before.0, "the model changed");
    assert_eq!(state.snapshot().revision, before.1, "a command was emitted");
}

/// S-F3 (#229): the native picker asks before overwriting the typed name;
/// the `.svg` extension added afterwards targets another file, which it has not
/// seen. If it exists, the export waits for confirmation: Cancel leaves it
/// intact, Replace overwrites it. Witness: a name with an extension waits for nothing.
#[test]
fn an_export_that_would_overwrite_an_unseen_file_asks_for_confirmation() {
    let dir = tempfile::tempdir().expect("temporary folder");
    let (_session, mut state, graph) = demo_with_graph(dir.path());
    let spec = Box::new(graph_spec(&state, graph));
    let existing = dir.path().join("figure.svg");
    std::fs::write(&existing, "ancien").unwrap();
    let exported = |state: &mut AppState, name: &str| {
        state.dispatch(Action::ExportGraph {
            path: dir.path().join(name),
            spec: spec.clone(),
        });
    };

    exported(&mut state, "figure");
    assert_eq!(state.pending_overwrite(), Some(existing.as_path()));
    assert!(state.blocking_dialog());
    state.dispatch(Action::ConfirmOverwrite(false));
    assert!(state.pending_overwrite().is_none());
    assert_eq!(std::fs::read_to_string(&existing).unwrap(), "ancien");

    exported(&mut state, "figure");
    state.dispatch(Action::ConfirmOverwrite(true));
    assert!(std::fs::read_to_string(&existing).unwrap().contains("<svg"));

    std::fs::write(&existing, "ancien").unwrap();
    exported(&mut state, "figure.svg");
    assert!(state.pending_overwrite().is_none(), "seen by the picker");
    assert!(std::fs::read_to_string(&existing).unwrap().contains("<svg"));
}

/// The format follows the extension: PDF and PNG (at the set magnification), `.svg`
/// added to a name without an extension, and an unknown extension is refused
/// with a message, no file written.
#[test]
fn the_export_format_follows_the_extension() {
    let dir = tempfile::tempdir().expect("temporary folder");
    let (_session, mut state, graph) = demo_with_graph(dir.path());
    let spec = Box::new(graph_spec(&state, graph));
    let exported = |state: &mut AppState, name: &str| {
        state.dispatch(Action::ExportGraph {
            path: dir.path().join(name),
            spec: spec.clone(),
        });
        state.status().expect("a message").clone()
    };

    assert!(!exported(&mut state, "figure.pdf").is_error());
    let pdf = std::fs::read(dir.path().join("figure.pdf")).expect("PDF");
    assert!(pdf.starts_with(b"%PDF"));

    state.dispatch(Action::SetPngExportScale(2.0));
    assert_eq!(state.png_export_scale(), 2.0);
    assert!(!exported(&mut state, "figure.png").is_error());
    let png = std::fs::read(dir.path().join("figure.png")).expect("PNG");
    assert_eq!(
        u32::from_be_bytes([png[16], png[17], png[18], png[19]]),
        960
    );
    // A magnification outside the proposed list is ignored.
    state.dispatch(Action::SetPngExportScale(3.0));
    assert_eq!(state.png_export_scale(), 2.0);

    assert!(!exported(&mut state, "sans_extension").is_error());
    assert!(dir.path().join("sans_extension.svg").exists());

    assert!(exported(&mut state, "figure.jpg").is_error());
    assert!(!dir.path().join("figure.jpg").exists());
}

/// #162: the SVG export keeps text as `<text>` by default, switches to
/// outlines (no `<text>`) when the user chooses so; the PDF and the PNG
/// do not depend on this setting, which enters neither the model nor the history.
#[test]
fn the_svg_text_mode_follows_the_preference_and_touches_neither_model_nor_history() {
    use stokesforge_render_svg::SvgTextMode;
    let dir = tempfile::tempdir().expect("temporary folder");
    let (_session, mut state, graph) = demo_with_graph(dir.path());
    let mut spec = graph_spec(&state, graph);
    spec.title = "Spectrum".to_owned();
    let spec = Box::new(spec);
    let revision = state.revision();
    let exported = |state: &mut AppState, name: &str| {
        state.dispatch(Action::ExportGraph {
            path: dir.path().join(name),
            spec: spec.clone(),
        });
        let status = state.status().expect("a message");
        assert!(!status.is_error(), "{}", status.text);
        std::fs::read_to_string(dir.path().join(name)).expect("SVG")
    };

    assert_eq!(state.svg_text_mode(), SvgTextMode::Text);
    let text = exported(&mut state, "texte.svg");
    assert!(text.contains("<text"), "SVG text expected by default");
    assert!(text.contains("Spectrum"));

    state.dispatch(Action::SetSvgTextMode(SvgTextMode::Outlines));
    assert_eq!(state.svg_text_mode(), SvgTextMode::Outlines);
    let contours = exported(&mut state, "contours.svg");
    assert!(!contours.contains("<text"), "no <text> in outlines");

    assert_eq!(state.revision(), revision, "a command was emitted");
}

#[test]
fn the_proposed_name_is_the_graph_name() {
    let dir = tempfile::tempdir().expect("temporary folder");
    let (_session, state, graph) = demo_with_graph(dir.path());
    assert_eq!(
        stokesforge_app_state::state::export_file_name(state.tree(), graph),
        "Graph1.svg"
    );
}

// -- Issue #35: free texts --

/// Add, move, remove a free text: each gesture is **one**
/// change of the model, so a single Ctrl+Z — whatever the number
/// of frames the edit or the drag lasted, whose preview lives in the
/// view. And a gesture that changes nothing stacks nothing.
#[test]
fn a_free_text_is_added_moved_and_removed_in_one_step_each() {
    use stokesforge_plot::{AnnotationRef, AnnotationSpec};
    let dir = tempfile::tempdir().expect("temporary folder");
    let (_session, mut state, graph) = demo_with_graph(dir.path());
    let target = AnnotationRef { panel: 0, index: 0 };
    let text = |state: &AppState| target.get(&graph_spec(state, graph)).cloned();
    let revision = |state: &AppState| state.snapshot().revision;

    let start = revision(&state);
    state.dispatch(Action::SetAnnotation {
        graph,
        target,
        annotation: Box::new(AnnotationSpec::new("Peak $lambda$", 1.0, 30.0)),
    });
    assert_eq!(
        text(&state).map(|a| a.text),
        Some("Peak $lambda$".to_owned())
    );
    let after_add = revision(&state);
    assert_ne!(after_add, start);

    // The same text: nothing changes, nothing is stacked.
    state.dispatch(Action::SetAnnotation {
        graph,
        target,
        annotation: Box::new(AnnotationSpec::new("Peak $lambda$", 1.0, 30.0)),
    });
    assert_eq!(
        revision(&state),
        after_add,
        "a no-effect gesture was stacked"
    );

    // Moved: a single entry, which a Ctrl+Z undoes entirely.
    state.dispatch(Action::SetAnnotation {
        graph,
        target,
        annotation: Box::new(AnnotationSpec::new("Peak $lambda$", 1.5, 40.0)),
    });
    assert_eq!(text(&state).map(|a| (a.x, a.y)), Some((1.5, 40.0)));
    state.dispatch(Action::Undo);
    assert_eq!(text(&state).map(|a| (a.x, a.y)), Some((1.0, 30.0)));

    state.dispatch(Action::RemoveAnnotation { graph, target });
    assert!(text(&state).is_none());
    state.dispatch(Action::Undo);
    assert!(text(&state).is_some(), "Ctrl+Z restores the removed text");
    state.dispatch(Action::Undo);
    assert!(text(&state).is_none(), "Ctrl+Z undoes the add");

    // An address beyond the last rank is ignored.
    state.dispatch(Action::SetAnnotation {
        graph,
        target: AnnotationRef { index: 4, ..target },
        annotation: Box::new(AnnotationSpec::new("x", 0.0, 0.0)),
    });
    assert!(graph_spec(&state, graph).panels[0].annotations.is_empty());
}

// -- Issue #76: graphic objects --

/// Add, move, stack and remove an object: each gesture is **one**
/// change of the model (a single Ctrl+Z), a gesture with no effect stacks nothing,
/// and bringing to front an object already on top does not either.
#[test]
fn a_graphic_object_is_added_stacked_and_removed_in_one_step_each() {
    use stokesforge_app_state::state::ShapeOrder;
    use stokesforge_plot::{ShapeKind, ShapeRef, ShapeSpec};
    let dir = tempfile::tempdir().expect("temporary folder");
    let (_session, mut state, graph) = demo_with_graph(dir.path());
    let at = |index| ShapeRef { panel: 0, index };
    let shapes = |state: &AppState| graph_spec(state, graph).panels[0].shapes.clone();
    let revision = |state: &AppState| state.snapshot().revision;
    let rect = |x: f64| ShapeSpec::new(ShapeKind::Rectangle, x, 0.0, x + 1.0, 1.0);

    for (index, x) in [(0, 1.0), (1, 5.0)] {
        state.dispatch(Action::SetShape {
            graph,
            target: at(index),
            shape: Box::new(rect(x)),
        });
    }
    assert_eq!(shapes(&state).len(), 2);

    // The same object: nothing is stacked.
    let after_add = revision(&state);
    state.dispatch(Action::SetShape {
        graph,
        target: at(1),
        shape: Box::new(rect(5.0)),
    });
    assert_eq!(revision(&state), after_add);

    // Already in front: nothing is stacked.
    state.dispatch(Action::ReorderShape {
        graph,
        target: at(1),
        order: ShapeOrder::ToFront,
    });
    assert_eq!(revision(&state), after_add);

    // To the front: the order changes, a Ctrl+Z undoes it.
    state.dispatch(Action::ReorderShape {
        graph,
        target: at(0),
        order: ShapeOrder::ToFront,
    });
    assert_eq!(
        shapes(&state).iter().map(|s| s.x1).collect::<Vec<_>>(),
        [5.0, 1.0]
    );
    state.dispatch(Action::Undo);
    assert_eq!(
        shapes(&state).iter().map(|s| s.x1).collect::<Vec<_>>(),
        [1.0, 5.0]
    );

    state.dispatch(Action::RemoveShape {
        graph,
        target: at(0),
    });
    assert_eq!(shapes(&state).len(), 1);
    state.dispatch(Action::Undo);
    assert_eq!(
        shapes(&state).len(),
        2,
        "Ctrl+Z restores the removed object"
    );

    // An address beyond the last rank is ignored.
    state.dispatch(Action::SetShape {
        graph,
        target: at(9),
        shape: Box::new(rect(0.0)),
    });
    assert_eq!(shapes(&state).len(), 2);
}

// -- Issue #53: movable titles, axis titles and legend --

/// Move a title, an axis title or the legend, then put them back: each
/// gesture is **one** change of the model (a single Ctrl+Z), and a gesture with no
/// effect stacks nothing.
/// Issues #77 and #78: the font and the size of a title, an axis title or
/// the legend are an attribute of the area, set in one undoable entry.
#[test]
fn the_font_of_a_text_area_is_set_in_one_undoable_step() {
    use stokesforge_plot::{AxisSide, TextFont, TextTarget};
    let dir = tempfile::tempdir().expect("temporary folder");
    let (_session, mut state, graph) = demo_with_graph(dir.path());
    let targets = [
        TextTarget::Title,
        TextTarget::AxisTitle {
            panel: 0,
            side: AxisSide::Left,
        },
        TextTarget::Legend { panel: 0 },
    ];
    let font = |state: &AppState, target: TextTarget| target.font(&graph_spec(state, graph));
    let wanted = TextFont {
        family: "Fira Sans".to_string(),
        size: Some(20.0),
        ..Default::default()
    };
    for target in targets {
        assert_eq!(font(&state, target), Some(TextFont::default()));
        state.dispatch(Action::SetTextFont {
            graph,
            target,
            font: wanted.clone(),
        });
        assert_eq!(font(&state, target), Some(wanted.clone()));
        let after = state.snapshot().revision;
        state.dispatch(Action::SetTextFont {
            graph,
            target,
            font: wanted.clone(),
        });
        assert_eq!(
            state.snapshot().revision,
            after,
            "an identical setting was stacked"
        );
        state.dispatch(Action::Undo);
        assert_eq!(font(&state, target), Some(TextFont::default()));
    }
}

#[test]
fn the_color_of_a_text_area_is_set_in_one_undoable_step() {
    use stokesforge_plot::{AxisSide, TextTarget};
    let dir = tempfile::tempdir().expect("temporary folder");
    let (_session, mut state, graph) = demo_with_graph(dir.path());
    let targets = [
        TextTarget::Title,
        TextTarget::PanelTitle { panel: 0 },
        TextTarget::AxisTitle {
            panel: 0,
            side: AxisSide::Left,
        },
        TextTarget::Legend { panel: 0 },
    ];
    let color = |state: &AppState, target: TextTarget| target.color(&graph_spec(state, graph));
    for target in targets {
        let before = color(&state, target);
        state.dispatch(Action::SetTextColor {
            graph,
            target,
            color: "#ff0000".to_string(),
        });
        assert_eq!(color(&state, target), Some("#ff0000".to_string()));
        state.dispatch(Action::Undo);
        assert_eq!(color(&state, target), before, "{target:?}");
    }
}

#[test]
fn a_title_is_moved_and_reset_in_one_step_each() {
    use stokesforge_plot::{AxisSide, TextAnchor, TextTarget};
    let dir = tempfile::tempdir().expect("temporary folder");
    let (_session, mut state, graph) = demo_with_graph(dir.path());
    let targets = [
        TextTarget::Title,
        TextTarget::AxisTitle {
            panel: 0,
            side: AxisSide::Left,
        },
        TextTarget::Legend { panel: 0 },
    ];
    let anchor = |state: &AppState, target: TextTarget| target.anchor(&graph_spec(state, graph));
    let revision = |state: &AppState| state.snapshot().revision;

    for target in targets {
        let before = revision(&state);
        state.dispatch(Action::SetTextAnchor {
            graph,
            target,
            anchor: Some(TextAnchor::new(1.0, 2.0)),
        });
        assert_eq!(anchor(&state, target), Some(TextAnchor::new(1.0, 2.0)));
        let after = revision(&state);
        assert_ne!(after, before);

        // The same anchor: nothing is stacked.
        state.dispatch(Action::SetTextAnchor {
            graph,
            target,
            anchor: Some(TextAnchor::new(1.0, 2.0)),
        });
        assert_eq!(revision(&state), after, "a no-effect gesture was stacked");

        // A single Ctrl+Z undoes the move.
        state.dispatch(Action::Undo);
        assert_eq!(anchor(&state, target), None);
        state.dispatch(Action::Redo);
        assert_eq!(anchor(&state, target), Some(TextAnchor::new(1.0, 2.0)));

        // "Reset Position" of a single text: one entry.
        state.dispatch(Action::SetTextAnchor {
            graph,
            target,
            anchor: None,
        });
        assert_eq!(anchor(&state, target), None);
        state.dispatch(Action::Undo);
        assert_eq!(anchor(&state, target), Some(TextAnchor::new(1.0, 2.0)));
    }

    // Reset everything at once: one entry for the whole.
    assert!(graph_spec(&state, graph).has_text_anchors());
    state.dispatch(Action::ResetTextAnchors { graph });
    assert!(!graph_spec(&state, graph).has_text_anchors());
    let after_reset = revision(&state);
    state.dispatch(Action::ResetTextAnchors { graph });
    assert_eq!(
        revision(&state),
        after_reset,
        "resetting everything when nothing is moved stacks nothing"
    );
    state.dispatch(Action::Undo);
    for target in targets {
        assert!(
            anchor(&state, target).is_some(),
            "{target:?} restored by Ctrl+Z"
        );
    }
}

/// Edit a title by double-click, rename a legend entry: one
/// change each, and an unchanged text freezes nothing.
#[test]
fn a_title_and_a_legend_name_are_edited_in_one_step() {
    use stokesforge_plot::{SeriesRef, TextTarget};
    let dir = tempfile::tempdir().expect("temporary folder");
    let (_session, mut state, graph) = demo_with_graph(dir.path());
    let title = TextTarget::AxisTitle {
        panel: 0,
        side: stokesforge_plot::AxisSide::Bottom,
    };
    state.dispatch(Action::SetTextContent {
        graph,
        target: title,
        text: "Time $t$".to_owned(),
    });
    assert_eq!(graph_spec(&state, graph).panels[0].bottom.label, "Time $t$");
    state.dispatch(Action::SetTextContent {
        graph,
        target: TextTarget::Title,
        text: "Courbe".to_owned(),
    });
    assert_eq!(graph_spec(&state, graph).title, "Courbe");
    // The legend has no text of its own: no effect.
    let before = state.snapshot().revision;
    state.dispatch(Action::SetTextContent {
        graph,
        target: TextTarget::Legend { panel: 0 },
        text: "x".to_owned(),
    });
    assert_eq!(state.snapshot().revision, before);

    let series = SeriesRef::new(0, 0);
    state.dispatch(Action::SetLegendEntryName {
        graph,
        series,
        name: "Échantillon".to_owned(),
    });
    assert_eq!(
        graph_spec(&state, graph)
            .series_at(series)
            .map(|s| s.name.clone()),
        Some("Échantillon".to_owned())
    );
    state.dispatch(Action::Undo);
    assert_eq!(
        graph_spec(&state, graph)
            .series_at(series)
            .map(|s| s.name.clone()),
        Some(String::new())
    );
    // A series that does not exist: ignored.
    let before = state.snapshot().revision;
    state.dispatch(Action::SetLegendEntryName {
        graph,
        series: SeriesRef::new(0, 99),
        name: "x".to_owned(),
    });
    assert_eq!(state.snapshot().revision, before);
}

#[test]
fn object_manager_group_visibility_is_undoable_and_persistent() {
    use stokesforge_app_state::object_manager::{object_tree, VisibilityTarget};
    use stokesforge_plot::{
        AnnotationSpec, PanelSpec, PlotSpec, SeriesRef, SeriesSpec, ShapeKind, ShapeSpec,
    };
    let dir = tempfile::tempdir().unwrap();
    let mut project = demo_project();
    let root = project.root();
    let book = project
        .tree()
        .child_by_name(root, "Book1")
        .unwrap()
        .unwrap();
    let sheet = project
        .tree()
        .child_by_name(book, "Sheet1")
        .unwrap()
        .unwrap();
    let column = project
        .tree()
        .child_by_name(sheet, "Signal")
        .unwrap()
        .unwrap();
    let mut first = PanelSpec::at(0, 0);
    first.series = vec![SeriesSpec::new(column), SeriesSpec::new(column)];
    first.series[1].name = "Reference".into();
    first
        .annotations
        .push(AnnotationSpec::new("Pic", 0.5, 10.0));
    first
        .shapes
        .push(ShapeSpec::new(ShapeKind::Rectangle, 0.0, 0.0, 1.0, 20.0));
    let mut second = PanelSpec::at(1, 0);
    second.series.push(SeriesSpec::new(column));
    let spec = PlotSpec::empty("Mesures").with_panels(vec![first, second]);
    let graph = project.add_graph(root, None, spec.clone()).unwrap();
    let (_session, mut state) = start(project, &dir.path().join("objets.stf"));
    let target = SeriesRef::new(0, 1);
    let tree = object_tree(state.tree(), graph, Some(target)).unwrap();
    assert_eq!(tree.layers.len(), 2);
    assert_eq!(tree.layers[0].series[1].label, "Reference");
    assert!(tree.layers[0].series[1].selected);
    assert!(!tree.layers[1].series[0].selected);

    state.dispatch(Action::SetPlotVisibility {
        graph,
        target: VisibilityTarget::Series(target),
        visible: false,
    });
    let hidden = graph_spec(&state, graph);
    assert!(!hidden.series_at(target).unwrap().visible);
    assert_eq!(hidden.series_at(target).unwrap().name, "Reference");
    assert!(VisibilityTarget::Group(0).visibility(&hidden).mixed);
    let revision = state.revision();
    state.dispatch(Action::SetPlotVisibility {
        graph,
        target: VisibilityTarget::Series(target),
        visible: false,
    });
    state.dispatch(Action::SetPlotVisibility {
        graph,
        target: VisibilityTarget::Series(SeriesRef::new(9, 9)),
        visible: false,
    });
    assert_eq!(
        state.revision(),
        revision,
        "neither an identical command, nor a stale target"
    );
    state.dispatch(Action::Undo);
    assert_eq!(graph_spec(&state, graph), spec);
    state.dispatch(Action::Redo);
    assert_eq!(graph_spec(&state, graph), hidden);

    state.dispatch(Action::SetPlotVisibility {
        graph,
        target: VisibilityTarget::Panel(0),
        visible: false,
    });
    let panel_hidden = graph_spec(&state, graph);
    assert!(!panel_hidden.panels[0].annotations[0].visible);
    assert!(!panel_hidden.panels[0].shapes[0].visible);
    assert!(
        panel_hidden.panels[1].series[0].visible,
        "the other layer stays visible"
    );
    state.dispatch(Action::Undo);
    assert_eq!(
        graph_spec(&state, graph),
        hidden,
        "the whole layer undoes in one entry"
    );
    state.dispatch(Action::Redo);
    state.dispatch(Action::Save);
    state.dispatch(Action::SetPlotVisibility {
        graph,
        target: VisibilityTarget::Graph,
        visible: true,
    });
    state.dispatch(Action::Load);
    assert_eq!(graph_spec(&state, graph), panel_hidden);
}

// -- Object Manager: gestures and non-series contents (#84–85) --

fn object_manager_project() -> (Project, NodeId, stokesforge_plot::PlotSpec) {
    use stokesforge_plot::{AnnotationSpec, PanelSpec, PlotSpec, SeriesSpec, ShapeKind, ShapeSpec};
    let mut project = Project::new("Objets").unwrap();
    let mut first = PanelSpec::at(0, 0);
    first.title = "Layer A".into();
    first.bottom.label = "Time".into();
    first.bottom.unit = "s".into();
    first.left.label = "Signal".into();
    for name in ["A", "B", "C"] {
        let mut series = SeriesSpec::new(NodeId::from_raw(900));
        series.name = name.into();
        first.series.push(series);
    }
    first.annotations.push(AnnotationSpec::new("Pic", 1.0, 2.0));
    for kind in [
        ShapeKind::Rectangle,
        ShapeKind::Ellipse,
        ShapeKind::Line,
        ShapeKind::Arrow,
    ] {
        first.shapes.push(ShapeSpec::new(kind, 0.0, 0.0, 1.0, 1.0));
    }
    let mut second = PanelSpec::at(1, 2);
    second.series = first.series.clone();
    second.series[1].visible = false;
    let spec = PlotSpec::empty("Titre").with_panels(vec![first, second]);
    let root = project.root();
    let graph = project.add_graph(root, None, spec.clone()).unwrap();
    (project, graph, spec)
}

/// #151: deleting a curve touches neither the shared data nor the
/// other curves; Cancel/Redo restores the graph and its projection.
#[test]
fn object_manager_deletes_a_curve_without_touching_the_data() {
    use stokesforge_app_state::object_manager::{object_tree, VisibilityTarget as T};
    use stokesforge_plot::{PanelSpec, PlotSpec, SeriesRef, SeriesSpec};
    let dir = tempfile::tempdir().unwrap();
    let mut project = demo_project();
    let sheet = project
        .tree()
        .iter()
        .find_map(|(id, node)| (node.name() == "Sheet1").then_some(id))
        .unwrap();
    let columns: Vec<_> = project.tree().children(sheet).unwrap().to_vec();
    let mut panel = PanelSpec::default();
    for name in ["First series", "Second series"] {
        let mut series = SeriesSpec::new(columns[1]);
        series.x = Some(columns[0]);
        series.name = name.into();
        panel.series.push(series);
    }
    let before = PlotSpec::default().with_panels(panel);
    let graph = project
        .add_graph(project.root(), None, before.clone())
        .unwrap();
    let (_session, mut state) = start(project, &dir.path().join("courbes.stf"));
    let original_nodes: Vec<_> = state
        .tree()
        .iter()
        .filter(|(id, _)| *id != graph)
        .map(|(id, node)| (id, node.clone()))
        .collect();
    state.dispatch(Action::RemovePlotObject {
        graph,
        target: T::Series(SeriesRef::new(0, 0)),
    });
    let mut after = before.clone();
    after.panels.get_mut(0).unwrap().series.remove(0);
    // The graph is legacy (nothing stored): the edit fixes the frames it is
    // drawn with (#394, G7); Undo gives the legacy spec back.
    let drawn = layout_of(&before);
    after.frame_widths = drawn.frame_widths;
    after.frame_heights = drawn.frame_heights;
    assert_eq!(graph_spec(&state, graph), after);
    let model = object_tree(state.tree(), graph, None).unwrap();
    assert_eq!(model.layers[0].series.len(), 1);
    assert_eq!(model.layers[0].series[0].label, "Second series");
    state.dispatch(Action::Undo);
    assert_eq!(graph_spec(&state, graph), before);
    assert_eq!(
        object_tree(state.tree(), graph, None).unwrap().layers[0]
            .series
            .len(),
        2
    );
    state.dispatch(Action::Redo);
    assert_eq!(graph_spec(&state, graph), after);
    let remaining_nodes: Vec<_> = state
        .tree()
        .iter()
        .filter(|(id, _)| *id != graph)
        .map(|(id, node)| (id, node.clone()))
        .collect();
    assert_eq!(remaining_nodes, original_nodes);
}

/// #344: the closed frame is a toggle in **a single** history entry
/// (`ClosedFrame` reason) that Undo undoes as a block; the bottom and
/// left tick marks are copied once; with a series on the right, only the top closes.
#[test]
fn the_closed_frame_is_a_single_history_entry_and_spares_the_busy_right_side() {
    use stokesforge_commands::PlotEditKind as K;
    use stokesforge_plot::{SeriesAxis, TickDirection};
    let dir = tempfile::tempdir().unwrap();
    let (mut project, graph, mut spec) = object_manager_project();
    {
        let panel = spec.panels.get_mut(0).unwrap();
        panel.bottom.tick_direction = TickDirection::In;
        panel.left.ticks = false;
        // The second layer has a series on the right axis: a real axis.
        let second = spec.panels.get_mut(1).unwrap();
        second.series[0].axis = SeriesAxis::Right;
    }
    project.set_plot_spec(graph, spec.clone()).unwrap();
    let (_session, mut state) = start(project, &dir.path().join("cadre.stf"));
    let labels = |state: &AppState| state.snapshot().history.undo_label.clone();

    state.dispatch(Action::SetClosedFrame {
        graph,
        panel: 0,
        closed: true,
    });
    let closed = graph_spec(&state, graph);
    let panel = &closed.panels[0];
    assert!(panel.frame_closed());
    assert!(panel.top.mirror && panel.right.mirror);
    assert!(!panel.top.tick_labels && !panel.right.tick_labels);
    assert_eq!(panel.top.tick_direction, TickDirection::In);
    assert!(
        panel.top.ticks && !panel.right.ticks,
        "tick marks copied from the bottom and left"
    );
    assert_eq!(
        labels(&state),
        Some(HistoryLabel::EditPlotSpec(K::ClosedFrame))
    );

    state.dispatch(Action::Undo);
    assert_eq!(
        graph_spec(&state, graph),
        spec,
        "one Undo restores everything"
    );
    state.dispatch(Action::Redo);
    assert_eq!(graph_spec(&state, graph), closed);

    // Toggle: the second gesture reopens the frame, in one entry too.
    state.dispatch(Action::SetClosedFrame {
        graph,
        panel: 0,
        closed: false,
    });
    let reopened = graph_spec(&state, graph);
    assert!(!reopened.panels[0].frame_closed());
    assert!(!reopened.panels[0].top.mirror && !reopened.panels[0].right.mirror);
    state.dispatch(Action::Undo);
    assert_eq!(
        graph_spec(&state, graph),
        closed,
        "a single Undo reopens the closed frame"
    );
    state.dispatch(Action::Redo);

    // Right occupied by a series: only the top closes, the right stays intact.
    state.dispatch(Action::SetClosedFrame {
        graph,
        panel: 1,
        closed: true,
    });
    let second = &graph_spec(&state, graph).panels[1];
    assert!(second.top.mirror && !second.top.tick_labels);
    assert_eq!(second.right, spec.panels[1].right);
    assert!(second.frame_closed());
}

/// #197: the gestures of a series group are undoable commands,
/// each with its history reason; a stale target creates none.
#[test]
fn the_gestures_of_a_series_group_are_undoable_and_persistent() {
    use stokesforge_app_state::object_manager::{object_tree, LayerSlot, VisibilityTarget as T};
    use stokesforge_commands::PlotEditKind as K;
    use stokesforge_plot::{SeriesGroupRef, SeriesRef};
    let dir = tempfile::tempdir().unwrap();
    let (mut project, graph, mut spec) = object_manager_project();
    // First layer: A, [B C] — group 1 of "B" and "C".
    {
        let panel = spec.panels.get_mut(0).unwrap();
        panel.series[1].group = Some(1);
        panel.series[2].group = Some(1);
    }
    project.set_plot_spec(graph, spec.clone()).unwrap();
    let (_session, mut state) = start(project, &dir.path().join("groupes.stf"));
    let group = SeriesGroupRef::new(0, 1);
    let labels = |state: &AppState| state.snapshot().history.undo_label.clone();

    state.dispatch(Action::RenameSeriesGroup {
        graph,
        group,
        name: "  Batches  ".into(),
    });
    let named = graph_spec(&state, graph);
    assert_eq!(named.panels[0].group_label(1), Some("Batches"));
    assert_eq!(
        labels(&state),
        Some(HistoryLabel::EditPlotSpec(K::RenameGroup))
    );
    let model = object_tree(state.tree(), graph, None).unwrap();
    assert!(matches!(
        &model.layers[0].slots[1],
        LayerSlot::Group(row) if row.name == "Batches" && row.members == [1, 2]
    ));

    let revision = state.revision();
    state.dispatch(Action::RenameSeriesGroup {
        graph,
        group,
        name: "Batches".into(),
    });
    state.dispatch(Action::RenameSeriesGroup {
        graph,
        group: SeriesGroupRef::new(0, 9),
        name: "Ghost".into(),
    });
    state.dispatch(Action::UngroupSeries {
        graph,
        group: SeriesGroupRef::new(0, 9),
    });
    state.dispatch(Action::MoveSeriesGroup {
        graph,
        group,
        up: false,
    });
    assert_eq!(
        state.revision(),
        revision,
        "identical name, missing group, limit: nothing"
    );

    // Moving the group up brings it before "A"; a lone series jumps over the group.
    state.dispatch(Action::MoveSeriesGroup {
        graph,
        group,
        up: true,
    });
    let moved = graph_spec(&state, graph);
    let names = |spec: &stokesforge_plot::PlotSpec| {
        spec.panels[0]
            .series
            .iter()
            .map(|s| s.name.clone())
            .collect::<Vec<_>>()
    };
    assert_eq!(names(&moved), ["B", "C", "A"]);
    assert_eq!(
        labels(&state),
        Some(HistoryLabel::EditPlotSpec(K::ReorderGroup))
    );
    state.dispatch(Action::MoveSeries {
        graph,
        series: SeriesRef::new(0, 2),
        up: true,
    });
    assert_eq!(names(&graph_spec(&state, graph)), ["A", "B", "C"]);
    assert_eq!(
        labels(&state),
        Some(HistoryLabel::EditPlotSpec(K::ReorderSeries))
    );
    state.dispatch(Action::MoveSeries {
        graph,
        series: SeriesRef::new(0, 1),
        up: true,
    });
    assert_eq!(
        names(&graph_spec(&state, graph)),
        ["A", "B", "C"],
        "first member of a group: it stays there"
    );

    // Dissolving keeps the series and their style; Undo gives back the group and its name.
    let before = graph_spec(&state, graph);
    state.dispatch(Action::UngroupSeries { graph, group });
    let ungrouped = graph_spec(&state, graph);
    assert_eq!(names(&ungrouped), names(&before));
    assert!(ungrouped.panels[0].series.iter().all(|s| s.group.is_none()));
    assert!(ungrouped.panels[0].group_names.is_empty());
    assert_eq!(
        labels(&state),
        Some(HistoryLabel::EditPlotSpec(K::DissolveGroup))
    );
    state.dispatch(Action::Undo);
    assert_eq!(graph_spec(&state, graph), before);

    // Deleting the group removes its series; the data are not touched.
    state.dispatch(Action::RemovePlotObject {
        graph,
        target: T::SeriesGroup(group),
    });
    assert_eq!(names(&graph_spec(&state, graph)), ["A"]);
    assert_eq!(
        labels(&state),
        Some(HistoryLabel::EditPlotSpec(K::RemoveGroup))
    );
    state.dispatch(Action::Undo);
    assert_eq!(graph_spec(&state, graph), before);
    state.dispatch(Action::Redo);
    assert_eq!(names(&graph_spec(&state, graph)), ["A"]);
    state.dispatch(Action::Undo);

    // The group's box only controls its members.
    state.dispatch(Action::SetPlotVisibility {
        graph,
        target: T::SeriesGroup(group),
        visible: false,
    });
    let hidden = graph_spec(&state, graph);
    assert_eq!(
        hidden.panels[0]
            .series
            .iter()
            .map(|s| s.visible)
            .collect::<Vec<_>>(),
        [true, false, false]
    );
}

#[test]
fn object_manager_series_rename_order_delete_undo_redo() {
    use stokesforge_app_state::object_manager::VisibilityTarget as T;
    use stokesforge_commands::PlotEditKind as K;
    use stokesforge_plot::SeriesRef;
    let dir = tempfile::tempdir().unwrap();
    let (project, graph, before) = object_manager_project();
    let (_session, mut state) = start(project, &dir.path().join("objets.stf"));
    let source = SeriesRef::new(1, 1); // Hidden series, second layer.
    state.dispatch(Action::SetLegendEntryName {
        graph,
        series: source,
        name: "Reference".into(),
    });
    let renamed = graph_spec(&state, graph);
    assert_eq!(renamed.series_at(source).unwrap().name, "Reference");
    assert!(!renamed.series_at(source).unwrap().visible);
    assert_eq!(
        state.snapshot().history.undo_label,
        Some(HistoryLabel::EditPlotSpec(K::RenameSeries))
    );
    state.dispatch(Action::Undo);
    assert_eq!(graph_spec(&state, graph), before);
    state.dispatch(Action::Redo);
    assert_eq!(graph_spec(&state, graph), renamed);

    state.dispatch(Action::PlaceSeries {
        graph,
        source,
        group: None,
        position: 2,
    });
    let moved = graph_spec(&state, graph);
    assert_eq!(
        moved.panels[1]
            .series
            .iter()
            .map(|s| s.name.as_str())
            .collect::<Vec<_>>(),
        ["A", "C", "Reference"]
    );
    assert_eq!(moved.panels[0], before.panels[0]);
    assert_eq!(
        state.snapshot().history.undo_label,
        Some(HistoryLabel::EditPlotSpec(K::ReorderSeries))
    );
    state.dispatch(Action::Undo);
    assert_eq!(graph_spec(&state, graph), renamed);
    state.dispatch(Action::Redo);
    assert_eq!(graph_spec(&state, graph), moved);

    let revision = state.revision();
    // Already in place, or stale target: no command.
    state.dispatch(Action::PlaceSeries {
        graph,
        source: SeriesRef::new(1, 2),
        group: None,
        position: 2,
    });
    state.dispatch(Action::PlaceSeries {
        graph,
        source: SeriesRef::new(9, 9),
        group: None,
        position: 0,
    });
    state.dispatch(Action::SetLegendEntryName {
        graph,
        series: SeriesRef::new(9, 9),
        name: "Absent".into(),
    });
    state.dispatch(Action::RemovePlotObject {
        graph,
        target: T::Series(SeriesRef::new(9, 9)),
    });
    assert_eq!(
        state.revision(),
        revision,
        "no command for a stale target or a no-effect gesture"
    );

    state.dispatch(Action::RemovePlotObject {
        graph,
        target: T::Series(SeriesRef::new(1, 2)),
    });
    assert_eq!(graph_spec(&state, graph).panels[1].series.len(), 2);
    assert_eq!(
        state.snapshot().history.undo_label,
        Some(HistoryLabel::EditPlotSpec(K::RemoveSeries))
    );
    state.dispatch(Action::Undo);
    assert_eq!(graph_spec(&state, graph), moved);
    state.dispatch(Action::Redo);
    for index in [1, 0] {
        state.dispatch(Action::RemovePlotObject {
            graph,
            target: T::Series(SeriesRef::new(1, index)),
        });
    }
    assert!(graph_spec(&state, graph).panels[1].series.is_empty());
    assert_eq!(
        graph_spec(&state, graph).panels.len(),
        2,
        "the last series does not delete the layer"
    );
}

/// Drag and drop of the Object Manager and the series window: a
/// command per gesture, undoable, whether it reorders, moves into a group
/// or out of one; a group is placed as a block; a free object is placed in
/// its category.
#[test]
fn object_manager_placement_of_series_groups_and_objects_is_undoable() {
    use stokesforge_app_state::object_manager::VisibilityTarget as T;
    use stokesforge_commands::PlotEditKind as K;
    use stokesforge_plot::{
        AnnotationRef, AnnotationSpec, PanelSpec, PlotSpec, SeriesGroupRef, SeriesRef, SeriesSpec,
        ShapeKind, ShapeRef, ShapeSpec,
    };
    let dir = tempfile::tempdir().unwrap();
    let mut project = Project::new("Placement").unwrap();
    let mut panel = PanelSpec::at(0, 0);
    for name in ["A", "B", "C", "D"] {
        let mut series = SeriesSpec::new(NodeId::from_raw(900));
        series.name = name.into();
        panel.series.push(series);
    }
    panel.series[1].group = Some(1);
    panel.series[2].group = Some(1);
    for text in ["t1", "t2"] {
        panel.annotations.push(AnnotationSpec::new(text, 0.0, 0.0));
    }
    for kind in [ShapeKind::Line, ShapeKind::Arrow] {
        panel.shapes.push(ShapeSpec::new(kind, 0.0, 0.0, 1.0, 1.0));
    }
    let spec = PlotSpec::empty("Titre").with_panels(vec![panel]);
    let root = project.root();
    let graph = project.add_graph(root, None, spec.clone()).unwrap();
    let (_session, mut state) = start(project, &dir.path().join("placement.stf"));
    let names = |spec: &PlotSpec| {
        spec.panels[0]
            .series
            .iter()
            .map(|s| s.name.clone())
            .collect::<Vec<_>>()
    };
    let label = |state: &AppState| state.snapshot().history.undo_label.clone();

    // Enter a group at the wanted rank: A joins [B C] between the two.
    state.dispatch(Action::PlaceSeries {
        graph,
        source: SeriesRef::new(0, 0),
        group: Some(1),
        position: 1,
    });
    let joined = graph_spec(&state, graph);
    assert_eq!(names(&joined), ["B", "A", "C", "D"]);
    assert_eq!(joined.panels[0].series[1].group, Some(1));
    assert_eq!(
        label(&state),
        Some(HistoryLabel::EditPlotSpec(K::ReorderSeries))
    );
    state.dispatch(Action::Undo);
    assert_eq!(
        graph_spec(&state, graph),
        spec,
        "a single Undo gives everything back"
    );

    // Leave a group for the head of the list.
    state.dispatch(Action::PlaceSeries {
        graph,
        source: SeriesRef::new(0, 2),
        group: None,
        position: 0,
    });
    let left = graph_spec(&state, graph);
    assert_eq!(names(&left), ["C", "A", "B", "D"]);
    assert_eq!(left.panels[0].series[0].group, None);
    state.dispatch(Action::Undo);

    // A group is placed as a block after the last lone series.
    state.dispatch(Action::PlaceSeriesGroup {
        graph,
        group: SeriesGroupRef::new(0, 1),
        position: 2,
    });
    assert_eq!(names(&graph_spec(&state, graph)), ["A", "D", "B", "C"]);
    assert_eq!(
        label(&state),
        Some(HistoryLabel::EditPlotSpec(K::ReorderGroup))
    );
    state.dispatch(Action::Undo);

    // A text is placed among the texts, a shape among the shapes.
    state.dispatch(Action::ReorderObject {
        graph,
        target: T::Annotation(AnnotationRef { panel: 0, index: 1 }),
        index: 0,
    });
    let texts = graph_spec(&state, graph);
    assert_eq!(texts.panels[0].annotations[0].text, "t2");
    assert_eq!(texts.panels[0].shapes, spec.panels[0].shapes);
    assert_eq!(
        label(&state),
        Some(HistoryLabel::EditPlotSpec(K::ReorderObject))
    );
    state.dispatch(Action::ReorderObject {
        graph,
        target: T::Shape(ShapeRef { panel: 0, index: 0 }),
        index: 1,
    });
    assert_eq!(
        graph_spec(&state, graph).panels[0].shapes[0].kind,
        ShapeKind::Arrow
    );

    // Nothing to do: no command.
    let revision = state.revision();
    state.dispatch(Action::PlaceSeries {
        graph,
        source: SeriesRef::new(0, 3),
        group: None,
        position: 3,
    });
    state.dispatch(Action::PlaceSeriesGroup {
        graph,
        group: SeriesGroupRef::new(0, 9),
        position: 0,
    });
    state.dispatch(Action::ReorderObject {
        graph,
        target: T::Series(SeriesRef::new(0, 0)),
        index: 1,
    });
    assert_eq!(state.revision(), revision);
}

#[test]
fn object_manager_properties_open_the_series_of_the_right_layer_and_undo() {
    use stokesforge_app_state::object_manager::VisibilityTarget as T;
    use stokesforge_plot::SeriesRef;
    let dir = tempfile::tempdir().unwrap();
    let (project, graph, before) = object_manager_project();
    let (_session, mut state) = start(project, &dir.path().join("objets.stf"));
    state.dispatch(Action::OpenObjectProperties {
        graph,
        target: T::Series(SeriesRef::new(1, 1)),
    });
    assert_eq!(state.plot_settings().unwrap().tab, PlotSettingsTab::Series);
    let edit = state.plot_edit_mut().unwrap();
    assert_eq!((edit.selected_panel, edit.selected_series), (1, 1));
    edit.spec.panels.get_mut(1).unwrap().series[1].line.width = 4.0;
    state.commit_plot_edit();
    assert_eq!(
        graph_spec(&state, graph).panels[1].series[1].line.width,
        4.0
    );
    state.cancel_plot_settings();
    assert_eq!(graph_spec(&state, graph), before);
    let revision = state.revision();
    state.dispatch(Action::OpenObjectProperties {
        graph,
        target: T::Series(SeriesRef::new(99, 0)),
    });
    assert!(state.plot_settings().is_none());
    assert_eq!(state.revision(), revision);
}

#[test]
fn object_manager_persistent_settings_and_reset_stay_outside_the_history() {
    use stokesforge_app_state::preferences::{
        ObjectManagerGroup, ObjectManagerSettings, Preferences,
    };
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("preferences.json");
    let (project, graph, before) = object_manager_project();
    let session = Session::spawn(project, None).unwrap();
    let mut state = AppState::new_with_preferences(
        session.handle(),
        dir.path().join("projet.stf").display().to_string(),
        Preferences::load_from(Some(path.clone())),
    )
    .unwrap();
    let defaults = ObjectManagerSettings::default();
    let revision = state.revision();
    state.dispatch(Action::SetObjectManagerOpen(false));
    for group in ObjectManagerGroup::ALL {
        state.dispatch(Action::SetObjectManagerGroupOpen {
            group,
            open: !defaults.group_open(group),
        });
        assert_eq!(
            state.object_manager_settings().group_open(group),
            !defaults.group_open(group)
        );
    }
    state.dispatch(Action::SetObjectManagerShowScales(true));
    let changed = state.object_manager_settings();
    assert_ne!(
        changed, defaults,
        "witness: the actions change the settings"
    );
    assert_eq!(
        Preferences::load_from(Some(path.clone())).object_manager_settings(),
        changed
    );
    assert!(state.object_manager_show_scales());
    state.dispatch(Action::ResetObjectManagerSettings);
    assert_eq!(state.object_manager_settings(), defaults);
    let loaded = Preferences::load_from(Some(path));
    assert_eq!(loaded.object_manager_settings(), defaults);
    assert!(
        !state.object_manager_open() && !loaded.object_manager_open(),
        "the reset does not reopen the panel"
    );
    assert_eq!(state.revision(), revision);
    assert!(!state.can_undo() && !state.can_redo());
    assert_eq!(graph_spec(&state, graph), before);
    assert!(state.status().is_none_or(|status| !status.is_error()));
    session.shutdown().unwrap();
}

#[test]
fn object_manager_setting_stays_active_if_storage_fails() {
    use stokesforge_app_state::preferences::{ObjectManagerGroup, Preferences};
    let dir = tempfile::tempdir().unwrap();
    // A folder in place of the file causes a portable error,
    // without depending on the permissions or the identity of the CI user.
    let session = Session::spawn(Project::new("Vide").unwrap(), None).unwrap();
    let mut state = AppState::new_with_preferences(
        session.handle(),
        dir.path().join("projet.stf").display().to_string(),
        Preferences::load_from(Some(dir.path().to_path_buf())),
    )
    .unwrap();
    let revision = state.revision();
    state.dispatch(Action::SetObjectManagerGroupOpen {
        group: ObjectManagerGroup::Graph,
        open: false,
    });
    assert!(!state.object_manager_settings().graph_open);
    assert!(state.status().is_some_and(|status| status.is_error()));
    assert_eq!(state.revision(), revision);
    assert!(!state.can_undo());
    session.shutdown().unwrap();
}

#[test]
fn object_manager_linked_texts_are_never_listed_without_a_reveal() {
    use stokesforge_app_state::object_manager::{
        object_tree_with_selection, VisibilityTarget as T,
    };
    use stokesforge_plot::{AnnotationRef, ShapeRef, TextTarget};
    let dir = tempfile::tempdir().unwrap();
    let (project, graph, before) = object_manager_project();
    let (_session, mut state) = start(project, &dir.path().join("cache.stf"));
    let text = AnnotationRef { panel: 0, index: 0 };
    let arrow = ShapeRef { panel: 0, index: 3 };
    // #214: neither graph title, nor layer title, nor axes, nor legend;
    // the free text and the four shapes stay.
    let tree = object_tree_with_selection(state.tree(), graph, None, false, true).unwrap();
    assert_eq!(tree.layers[0].objects.len(), 5);
    assert!(tree.layers[0]
        .objects
        .iter()
        .all(|o| matches!(o.target, T::Annotation(_) | T::Shape(_))));
    assert!(tree.layers[0]
        .objects
        .iter()
        .any(|o| o.target == T::Annotation(text)));
    assert!(tree.layers[0]
        .objects
        .iter()
        .any(|o| o.target == T::Shape(arrow)));
    assert!(!tree.layers[0].series.is_empty(), "series always listed");
    // No reveal on selection: a selected linked text does not appear.
    let selected = object_tree_with_selection(
        state.tree(),
        graph,
        Some(T::Text(TextTarget::Title)),
        false,
        true,
    )
    .unwrap();
    assert!(selected.layers[0].objects.iter().all(|o| !o.selected));
    // #173: the graph's box only controls what the tree shows; the
    // linked texts keep their state, and do not make the box indeterminate.
    state.dispatch(Action::SetPlotVisibility {
        graph,
        target: T::Graph,
        visible: false,
    });
    let spec = graph_spec(&state, graph);
    assert!(spec.title_visible);
    assert!(spec.panels[0].bottom.title_visible);
    assert!(!T::Graph.visibility(&spec).checked);
    assert!(!T::Graph.visibility(&spec).mixed);
    assert!(!spec.panels[0].series[0].visible);
    state.dispatch(Action::Undo);
    assert_eq!(graph_spec(&state, graph), before);
}

#[test]
fn object_manager_show_objects_lists_or_hides_the_free_objects() {
    use stokesforge_app_state::object_manager::{
        object_tree_with_selection, VisibilityTarget as T,
    };
    let dir = tempfile::tempdir().unwrap();
    let (project, graph, before) = object_manager_project();
    let (_session, mut state) = start(project, &dir.path().join("objets.stf"));
    let listed = object_tree_with_selection(state.tree(), graph, None, false, true).unwrap();
    assert_eq!(listed.layers[0].objects.len(), 5);
    let hidden = object_tree_with_selection(state.tree(), graph, None, false, false).unwrap();
    assert!(hidden.layers[0].objects.is_empty());
    assert!(!hidden.layers[0].series.is_empty());
    // Unlisted objects do not count in the graph's box (#173).
    state.dispatch(Action::SetObjectManagerShowObjects(false));
    state.dispatch(Action::SetPlotVisibility {
        graph,
        target: T::Graph,
        visible: false,
    });
    let spec = graph_spec(&state, graph);
    assert!(spec.panels[0].annotations.iter().all(|a| a.visible));
    assert!(!spec.panels[0].series[0].visible);
    state.dispatch(Action::Undo);
    assert_eq!(graph_spec(&state, graph), before);
}

#[test]
fn object_manager_scales_hidden_by_default_one_box_per_axis() {
    use stokesforge_app_state::object_manager::{
        object_tree_with_selection, AxisRef, ObjectKind, VisibilityTarget as T,
    };
    use stokesforge_plot::AxisSide;
    let dir = tempfile::tempdir().unwrap();
    let (project, graph, before) = object_manager_project();
    let (_session, mut state) = start(project, &dir.path().join("cache.stf"));
    let axes = |tree: &stokesforge_app_state::object_manager::ObjectTree| -> Vec<T> {
        tree.layers[0]
            .axes
            .iter()
            .filter(|o| matches!(o.kind, ObjectKind::Axis))
            .map(|o| o.target)
            .collect()
    };
    // Hidden by default, even with the texts displayed.
    let hidden = object_tree_with_selection(state.tree(), graph, None, false, true).unwrap();
    assert!(axes(&hidden).is_empty());
    // Active toggle: the "Axes" group and its four rows, including the top and
    // the right, mirror axes (#176); the other objects of the layer contain
    // none.
    let shown = object_tree_with_selection(state.tree(), graph, None, true, true).unwrap();
    let axis = |side| T::Axis(AxisRef { panel: 0, side });
    assert_eq!(
        axes(&shown),
        [
            AxisSide::Bottom,
            AxisSide::Left,
            AxisSide::Right,
            AxisSide::Top
        ]
        .map(axis)
    );
    assert!(shown.layers[0]
        .objects
        .iter()
        .all(|o| !matches!(o.kind, ObjectKind::Axis)));
    // One box per axis: `AxisSpec::visible`, a single history entry.
    let left = axis(AxisSide::Left);
    assert!(left.visibility(&before).checked);
    // The top and right mirrors are unchecked by default (#176): the
    // group is mixed from the start.
    assert!(T::Axes(0).visibility(&before).mixed);
    state.dispatch(Action::SetPlotVisibility {
        graph,
        target: left,
        visible: false,
    });
    let spec = graph_spec(&state, graph);
    assert!(!spec.panels[0].left.visible);
    assert!(!left.visibility(&spec).checked);
    // The group's box is mixed as soon as one of its axes is hidden, and it
    // controls them all in one write.
    assert!(T::Axes(0).visibility(&spec).mixed);
    state.dispatch(Action::SetPlotVisibility {
        graph,
        target: T::Axes(0),
        visible: true,
    });
    assert!(T::Axes(0).visibility(&graph_spec(&state, graph)).checked);
    state.dispatch(Action::Undo);
    state.dispatch(Action::Undo);
    assert_eq!(graph_spec(&state, graph), before);
    // The graph's and the layer's boxes do not touch the axes.
    state.dispatch(Action::SetPlotVisibility {
        graph,
        target: T::Graph,
        visible: true,
    });
    let spec = graph_spec(&state, graph);
    for side in [AxisSide::Bottom, AxisSide::Right, AxisSide::Top] {
        assert_eq!(
            spec.panels[0].axis(side).visible,
            before.panels[0].axis(side).visible
        );
    }
}

#[test]
fn object_manager_free_objects_tree_visibility_and_file() {
    use stokesforge_app_state::object_manager::{
        object_tree_with_selection, VisibilityTarget as T,
    };
    use stokesforge_plot::{AnnotationRef, AxisSide, ShapeRef, TextTarget};
    let dir = tempfile::tempdir().unwrap();
    let (project, graph, before) = object_manager_project();
    let (_session, mut state) = start(project, &dir.path().join("objets.stf"));
    let text = AnnotationRef { panel: 0, index: 0 };
    let arrow = ShapeRef { panel: 0, index: 3 };
    let model = object_tree_with_selection(state.tree(), graph, Some(T::Shape(arrow)), false, true)
        .unwrap();
    assert_eq!(
        model.layers[0].objects.len(),
        5,
        "free text and four shapes, no linked text (#214)"
    );
    assert_eq!(
        model.layers[0]
            .objects
            .iter()
            .filter(|o| o.selected)
            .count(),
        1
    );
    assert!(model.layers[0]
        .objects
        .iter()
        .any(|o| o.target == T::Shape(arrow) && o.selected));
    let target = TextTarget::AxisTitle {
        panel: 0,
        side: AxisSide::Bottom,
    };
    state.dispatch(Action::SetPlotVisibility {
        graph,
        target: T::Text(target),
        visible: false,
    });
    let hidden_title = graph_spec(&state, graph);
    assert_eq!(target.text(&hidden_title).as_deref(), Some("Time (s)"));
    assert_eq!(target.visible(&hidden_title), Some(false));
    assert!(
        !T::Panel(0).visibility(&hidden_title).mixed,
        "a linked title is not listed: it does not count in the layer's box"
    );
    state.dispatch(Action::Undo);
    assert_eq!(graph_spec(&state, graph), before);

    state.dispatch(Action::SetPlotVisibility {
        graph,
        target: T::Graph,
        visible: false,
    });
    let hidden = graph_spec(&state, graph);
    assert!(
        hidden.title_visible && hidden.panels[0].bottom.title_visible,
        "unlisted linked texts keep their state (#214)"
    );
    assert!(hidden.panels[0].bottom.visible);
    assert!(!text.get(&hidden).unwrap().visible);
    assert!(!arrow.get(&hidden).unwrap().visible);
    assert!(!T::Graph.visibility(&hidden).checked);
    assert!(!T::Graph.visibility(&hidden).mixed);
    let model =
        object_tree_with_selection(state.tree(), graph, Some(T::Annotation(text)), false, true)
            .unwrap();
    assert!(model.layers[0]
        .objects
        .iter()
        .any(|o| o.target == T::Annotation(text) && o.selected));
    state.dispatch(Action::Save);
    state.dispatch(Action::SetPlotVisibility {
        graph,
        target: T::Graph,
        visible: true,
    });
    state.dispatch(Action::Load);
    assert_eq!(graph_spec(&state, graph), hidden);

    for target in [
        T::Annotation(text),
        T::Shape(arrow),
        T::Text(TextTarget::Title),
    ] {
        let before = graph_spec(&state, graph);
        state.dispatch(Action::RemovePlotObject { graph, target });
        assert_ne!(graph_spec(&state, graph), before);
        state.dispatch(Action::Undo);
        assert_eq!(graph_spec(&state, graph), before);
        state.dispatch(Action::Redo);
        assert_ne!(graph_spec(&state, graph), before);
        state.dispatch(Action::Undo);
    }
}

// -- Sheets: rename, duplicate, delete (#106) --

/// Rename a sheet: a single history entry, undoable; an empty name
/// or one already taken in the workbook → the old name stays, error message, nothing
/// in the history.
#[test]
fn renaming_a_sheet_is_undoable_and_rejects_empty_or_taken_names() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let book = book_named(state.tree(), "Book1");
    state.dispatch(Action::AddWorksheet { book });
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let other = state.worksheets_of(book).last().unwrap().clone();

    state.dispatch(Action::BeginRenameSheet(sheet1));
    assert_eq!(state.renaming_sheet(), Some(sheet1));
    state.dispatch(Action::CommitRenameSheet {
        sheet: sheet1,
        name: "Mesures".to_string(),
    });
    assert_eq!(state.renaming_sheet(), None);
    assert_eq!(state.tree().node(sheet1).unwrap().name(), "Mesures");
    state.dispatch(Action::Undo);
    assert_eq!(state.tree().node(sheet1).unwrap().name(), "Sheet1");

    let revision = state.revision();
    for name in ["   ", other.1.as_str()] {
        state.dispatch(Action::BeginRenameSheet(sheet1));
        state.dispatch(Action::CommitRenameSheet {
            sheet: sheet1,
            name: name.to_string(),
        });
        assert_eq!(state.tree().node(sheet1).unwrap().name(), "Sheet1");
        assert!(state.status().is_some_and(|status| status.is_error()));
        assert_eq!(state.revision(), revision, "no history entry");
    }

    state.dispatch(Action::BeginRenameSheet(sheet1));
    state.dispatch(Action::CancelRenameSheet);
    assert_eq!(state.renaming_sheet(), None);
}

/// Duplicate: "… copy 1", "… copy 2" (first free number), columns
/// with new identifiers and the same values, copy displayed; the mother's
/// graphs still plot it; a Ctrl+Z removes the copy.
#[test]
fn duplicating_a_sheet_copies_its_columns_and_keeps_graphs_on_the_original() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let book = book_named(state.tree(), "Book1");
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let time = column_id(&state, sheet1, "Time");
    let signal = column_id(&state, sheet1, "Signal");
    state.dispatch(Action::PlotColumns {
        sheet: sheet1,
        columns: vec![time, signal],
        plot_type: PlotType::Line,
    });
    let before = std::sync::Arc::clone(state.tree());
    let count = state.worksheets_of(book).len();

    state.dispatch(Action::DuplicateSheet(sheet1));
    state.dispatch(Action::DuplicateSheet(sheet1));

    let sheets = state.worksheets_of(book);
    assert_eq!(sheets.len(), count + 2);
    let names: Vec<&str> = sheets.iter().map(|(_, n)| n.as_str()).collect();
    assert!(names.contains(&"Sheet1 copy 1"), "{names:?}");
    assert!(names.contains(&"Sheet1 copy 2"), "{names:?}");
    let copy2 = sheet_named(state.tree(), "Sheet1 copy 2");
    assert_eq!(state.selected_sheet_in(book), Some(copy2));
    let copy1 = sheet_named(state.tree(), "Sheet1 copy 1");
    let original: Vec<_> = state.tree().node(sheet1).unwrap().children().to_vec();
    let copied: Vec<_> = state.tree().node(copy1).unwrap().children().to_vec();
    assert_eq!(original.len(), copied.len());
    for (a, b) in original.iter().zip(&copied) {
        assert_ne!(a, b, "new identifiers");
        assert_eq!(
            state.tree().node(*a).unwrap().name(),
            state.tree().node(*b).unwrap().name()
        );
        assert_eq!(
            state.tree().node(*a).unwrap().as_column(),
            state.tree().node(*b).unwrap().as_column()
        );
    }
    assert_eq!(state.sheet_users(copy1), Vec::<String>::new());
    assert_eq!(state.sheet_users(sheet1).len(), 1);

    state.dispatch(Action::Undo);
    state.dispatch(Action::Undo);
    assert_eq!(**state.tree(), *before);
}

/// Deleting a sheet asks nothing (S-F3, #229): the message names the
/// graphs that plotted it, Undo gives it back; the last sheet of a workbook
/// is refused.
#[test]
fn deleting_a_sheet_needs_no_confirmation_and_keeps_the_last_sheet() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let book = book_named(state.tree(), "Book1");
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let time = column_id(&state, sheet1, "Time");
    let signal = column_id(&state, sheet1, "Signal");
    state.dispatch(Action::PlotColumns {
        sheet: sheet1,
        columns: vec![time, signal],
        plot_type: PlotType::Line,
    });
    let only = state.worksheets_of(book);
    if only.len() == 1 {
        assert!(!state.can_delete_sheet(sheet1));
        state.dispatch(Action::DeleteSheet(sheet1));
        assert!(state.tree().node(sheet1).is_some());
        assert!(state.status().is_some_and(|status| status.is_error()));
    }
    state.dispatch(Action::AddWorksheet { book });
    let before = std::sync::Arc::clone(state.tree());

    assert!(state.can_delete_sheet(sheet1));
    let users = state.sheet_users(sheet1);
    assert_eq!(users.len(), 1);
    state.dispatch(Action::DeleteSheet(sheet1));
    assert!(state.tree().node(sheet1).is_none());
    assert!(!state.blocking_dialog());
    let status = state.status().expect("message");
    assert!(!status.is_error());
    assert!(status.text.contains(&users[0]), "{}", status.text);
    state.dispatch(Action::Undo);
    assert_eq!(**state.tree(), *before);
}

// -- Issue #149: copy the figure to an external application --

/// The copy prepares SVG, PNG then PDF of the figure — in that order, that of
/// the preference — without touching the model: neither the `PlotSpec` nor the revision
/// moves, so nothing to undo. The PNG follows the magnification set for
/// the exports, and the representation is picked up only once.
#[test]
fn copying_the_figure_prepares_svg_png_pdf_without_touching_the_model() {
    use stokesforge_render_svg::ExportFormat;
    let dir = tempfile::tempdir().expect("temporary folder");
    let (_session, mut state, graph) = demo_with_graph(dir.path());
    let before = (graph_spec(&state, graph), state.snapshot().revision);
    state.dispatch(Action::SetPngExportScale(2.0));

    state.dispatch(Action::CopyGraphFigure {
        spec: Box::new(before.0.clone()),
    });
    let status = state.status().expect("a message");
    assert!(!status.is_error(), "{}", status.text);
    assert_eq!(
        status.text,
        stokesforge_app_state::tr!("status-graph-copied", formats = "SVG, PNG, PDF")
    );
    let reps = state.take_figure_clipboard().expect("the figure is ready");
    assert!(matches!(
        reps.iter().map(|r| r.format).collect::<Vec<_>>()[..],
        [
            ExportFormat::Svg { .. },
            ExportFormat::Png { .. },
            ExportFormat::Pdf
        ]
    ));
    assert!(String::from_utf8_lossy(&reps[0].bytes).contains("<svg"));
    assert!(reps[1].bytes.starts_with(b"\x89PNG"));
    // Witness of the setting: the PNG is at scale 2 (480 × 2).
    assert_eq!(
        u32::from_be_bytes([
            reps[1].bytes[16],
            reps[1].bytes[17],
            reps[1].bytes[18],
            reps[1].bytes[19]
        ]),
        960
    );
    assert!(reps[2].bytes.starts_with(b"%PDF"));
    assert!(state.take_figure_clipboard().is_none(), "picked up once");

    assert_eq!(graph_spec(&state, graph), before.0, "the model changed");
    assert_eq!(state.snapshot().revision, before.1, "a command was emitted");
    assert!(!state.can_undo(), "no history entry");
}

/// Preferences › Export sets the copy: the order of the representations is that
/// of the list, an unchecked format is not offered, and the message only names
/// those that are.
#[test]
fn copying_the_figure_follows_the_order_and_the_choice_of_the_preferences() {
    use stokesforge_app_state::preferences::{CopyFormat, CopyFormatEntry};
    use stokesforge_render_svg::ExportFormat;
    let dir = tempfile::tempdir().expect("temporary folder");
    let (_session, mut state, graph) = demo_with_graph(dir.path());
    state.dispatch(Action::SetCopyFormats(vec![
        CopyFormatEntry {
            format: CopyFormat::Pdf,
            on: true,
        },
        CopyFormatEntry {
            format: CopyFormat::Svg,
            on: false,
        },
        CopyFormatEntry {
            format: CopyFormat::Png,
            on: true,
        },
    ]));
    state.dispatch(Action::CopyGraphFigure {
        spec: Box::new(graph_spec(&state, graph)),
    });
    let status = state.status().expect("a message");
    assert!(!status.is_error(), "{}", status.text);
    assert_eq!(
        status.text,
        stokesforge_app_state::tr!("status-graph-copied", formats = "PDF, PNG")
    );
    let reps = state.take_figure_clipboard().expect("the figure is ready");
    assert!(matches!(
        reps.iter().map(|r| r.format).collect::<Vec<_>>()[..],
        [ExportFormat::Pdf, ExportFormat::Png { .. }]
    ));
}

/// The copied figure is that of the framed spec given: a zoom changes the
/// bytes (same witness as the export to a file).
#[test]
fn copying_a_zoomed_figure_copies_what_is_seen() {
    let dir = tempfile::tempdir().expect("temporary folder");
    let (_session, mut state, graph) = demo_with_graph(dir.path());
    let whole = graph_spec(&state, graph);
    let mut zoomed = whole.clone();
    if let Some(panel) = zoomed.panels.get_mut(0) {
        panel.bottom.auto_range = false;
        panel.bottom.range_min = Some(0.5);
        panel.bottom.range_max = Some(1.0);
    }
    let svg = |state: &mut AppState, spec: &stokesforge_plot::PlotSpec| {
        state.dispatch(Action::CopyGraphFigure {
            spec: Box::new(spec.clone()),
        });
        state.take_figure_clipboard().expect("ready")[0]
            .bytes
            .clone()
    };
    assert_ne!(svg(&mut state, &zoomed), svg(&mut state, &whole));
}

/// A system refusal replaces the "copied" message with its cause. The sentence
/// is put by the window (`clipboard_error_message`, tested in
/// `app/figure_copy.rs`); the state shows it as is, as an error.
#[test]
fn a_clipboard_refusal_replaces_the_copied_message() {
    let dir = tempfile::tempdir().expect("temporary folder");
    let (_session, mut state, graph) = demo_with_graph(dir.path());
    state.dispatch(Action::CopyGraphFigure {
        spec: Box::new(graph_spec(&state, graph)),
    });
    assert!(!state.status().expect("message").is_error());
    // The sentence comes from the window (`clipboard_error_message`, tested in
    // `app/figure_copy.rs`): the state only shows it as an error.
    state.figure_clipboard_failed("Could not copy the graph: selection refused".into());
    let status = state.status().expect("message");
    assert!(status.is_error());
    assert_eq!(status.text, "Could not copy the graph: selection refused");
}

/// #153: a resize goes through the same action as the insertion;
/// a single undo/redo restores the dimensions and the whole style.
#[test]
fn resizing_a_shape_is_undoable_in_one_step() {
    use stokesforge_plot::{ShapeKind, ShapeRef, ShapeSpec};
    let dir = tempfile::tempdir().unwrap();
    let (_session, mut state, graph) = demo_with_graph(dir.path());
    let target = ShapeRef { panel: 0, index: 0 };
    for kind in [
        ShapeKind::Rectangle,
        ShapeKind::Ellipse,
        ShapeKind::Line,
        ShapeKind::Arrow,
    ] {
        let original = ShapeSpec::new(kind, 1.0, 2.0, 5.0, 4.0);
        state.dispatch(Action::SetShape {
            graph,
            target,
            shape: Box::new(original.clone()),
        });
        let resized = ShapeSpec {
            x2: 9.0,
            y2: 6.0,
            ..original.clone()
        };
        state.dispatch(Action::SetShape {
            graph,
            target,
            shape: Box::new(resized.clone()),
        });
        assert_eq!(target.get(&graph_spec(&state, graph)), Some(&resized));
        state.dispatch(Action::Undo);
        assert_eq!(target.get(&graph_spec(&state, graph)), Some(&original));
        state.dispatch(Action::Redo);
        assert_eq!(target.get(&graph_spec(&state, graph)), Some(&resized));
    }
}

// -- 20. Multiple column selection: Ctrl+click, Shift+click (#159) --

/// Ctrl+click adds then removes a column, one at a time, without requiring
/// that it touch the others; the order of the actions is that of the sheet.
#[test]
fn ctrl_click_toggles_non_contiguous_columns() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let time = column_id(&state, sheet1, "Time");
    let signal = column_id(&state, sheet1, "Signal");
    let note = column_id(&state, sheet1, "Note");

    state.select_column(sheet1, note);
    state.toggle_column_selection(sheet1, time);

    assert!(state.is_column_selected(sheet1, time));
    assert!(!state.is_column_selected(sheet1, signal));
    assert!(state.is_column_selected(sheet1, note));
    assert_eq!(
        state
            .selection(sheet1)
            .unwrap()
            .columns(state.tree(), sheet1),
        vec![time, note],
        "the sheet order, not the click order"
    );

    state.toggle_column_selection(sheet1, note);
    assert!(!state.is_column_selected(sheet1, note));
    assert!(state.is_column_selected(sheet1, time));
}

/// Shift+click replaces everything with the anchor → target range; Ctrl+Shift+click
/// (`extend_column_selection`) adds it to the columns already set.
#[test]
fn shift_click_replaces_and_ctrl_shift_click_adds_a_range() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let columns: Vec<NodeId> = state
        .visible_columns(sheet1)
        .into_iter()
        .map(|(id, _, _)| id)
        .collect();
    assert!(columns.len() >= 3);
    let (first, last) = (columns[0], columns[columns.len() - 1]);

    state.select_column(sheet1, first);
    state.toggle_column_selection(sheet1, last);
    // Shift+click: the anchor (`last`) → `first` range replaces both.
    state.extend_column_selection_replacing(sheet1, first);
    for column in &columns {
        assert!(state.is_column_selected(sheet1, *column));
    }

    state.select_column(sheet1, first);
    state.toggle_column_selection(sheet1, last);
    // Ctrl+Shift+click: anchor `last` → next column, the old ones stay.
    state.extend_column_selection(sheet1, columns[columns.len() - 2]);
    assert!(state.is_column_selected(sheet1, first));
    assert!(state.is_column_selected(sheet1, columns[columns.len() - 2]));
    assert!(state.is_column_selected(sheet1, last));
}

/// A non-contiguous selection disables Paste and Paste in Place (ambiguous
/// target) and keyboard paste is refused with a message; a
/// contiguous selection keeps both.
#[test]
fn a_non_contiguous_selection_cannot_be_pasted_into() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let time = column_id(&state, sheet1, "Time");
    let signal = column_id(&state, sheet1, "Signal");
    let note = column_id(&state, sheet1, "Note");

    state.select_column(sheet1, time);
    state.toggle_column_selection(sheet1, signal);
    assert!(state.grid_menu(sheet1).unwrap().paste_enabled);

    state.select_column(sheet1, time);
    state.toggle_column_selection(sheet1, note);
    assert!(!state.grid_menu(sheet1).unwrap().paste_enabled);

    let before = cell(&state, time, 0);
    state.dispatch(Action::Paste {
        sheet: sheet1,
        text: "999".to_string(),
    });
    assert_eq!(cell(&state, time, 0), before, "nothing is pasted");
}

/// Copying a non-contiguous selection puts the columns side by side, in
/// sheet order.
#[test]
fn copying_non_contiguous_columns_joins_them_in_sheet_order() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let time = column_id(&state, sheet1, "Time");
    let signal = column_id(&state, sheet1, "Signal");
    let note = column_id(&state, sheet1, "Note");

    state.select_column(sheet1, note);
    state.toggle_column_selection(sheet1, time);
    let text = state.copy_selection(sheet1).expect("non-empty selection");

    state.select_column(sheet1, time);
    state.toggle_column_selection(sheet1, signal);
    state.toggle_column_selection(sheet1, note);
    let all = state.copy_selection(sheet1).expect("non-empty selection");

    let first_line = text.lines().next().unwrap();
    assert_eq!(first_line.split('\t').count(), 2, "two columns: {text:?}");
    assert_eq!(all.lines().next().unwrap().split('\t').count(), 3);
    assert_eq!(
        first_line.split('\t').next(),
        all.lines().next().unwrap().split('\t').next(),
        "Time first in both copies"
    );
}

/// A column removed from the model leaves the multiple selection.
#[test]
fn toggling_a_column_off_keeps_the_others() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let time = column_id(&state, sheet1, "Time");
    let signal = column_id(&state, sheet1, "Signal");

    state.select_column(sheet1, time);
    state.toggle_column_selection(sheet1, signal);
    state.toggle_column_selection(sheet1, time);

    assert!(!state.is_column_selected(sheet1, time));
    assert!(state.is_column_selected(sheet1, signal));
    // Ctrl+click on a cell of another kind: a simple selection.
    state.select_cell(sheet1, time, 0);
    state.toggle_column_selection(sheet1, signal);
    assert!(state.is_column_selected(sheet1, signal));
    assert!(!state.is_column_selected(sheet1, time));
}

// -- 21. Multiple selection of rows and cells (#159) --

/// Removing a cell from the middle of a rectangle cuts it around it;
/// putting it back adds it again. The neighboring cells stay selected.
#[test]
fn toggling_a_cell_out_of_a_rectangle_splits_it() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let time = column_id(&state, sheet1, "Time");
    let signal = column_id(&state, sheet1, "Signal");

    state.select_cell(sheet1, time, 0);
    state.extend_selection(sheet1, signal, 2);
    state.toggle_cell_selection(sheet1, time, 1);

    assert!(!state.is_selected(sheet1, time, 1));
    for (column, row) in [(time, 0), (time, 2), (signal, 0), (signal, 1), (signal, 2)] {
        assert!(state.is_selected(sheet1, column, row), "{row}");
    }
    state.toggle_cell_selection(sheet1, time, 1);
    assert!(state.is_selected(sheet1, time, 1));
}

/// Only one kind at a time: a Ctrl+click on a header field when data
/// cells are selected sets a simple selection again.
#[test]
fn ctrl_click_never_mixes_header_and_data() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let time = column_id(&state, sheet1, "Time");

    state.select_cell(sheet1, time, 0);
    state.toggle_header_field_selection(sheet1, time, HeaderField::Unit);

    assert!(!state.is_selected(sheet1, time, 0));
    assert!(state.is_header_field_selected(sheet1, time, HeaderField::Unit));
}

/// Two cells that share their row are copied side by side; two
/// cells that share neither row nor column are not copied (Copy and
/// Cut disabled).
#[test]
fn copying_multiple_cells_needs_shared_rows_or_columns() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let time = column_id(&state, sheet1, "Time");
    let signal = column_id(&state, sheet1, "Signal");

    state.select_cell(sheet1, time, 1);
    state.toggle_cell_selection(sheet1, signal, 1);
    assert!(state.grid_menu(sheet1).unwrap().copy_enabled);
    let side_by_side = state.copy_selection(sheet1).expect("same row");
    assert_eq!(side_by_side.lines().count(), 1);
    assert_eq!(side_by_side.lines().next().unwrap().split('\t').count(), 2);

    // Same column, rows 0 and 2: stacked, without row 1.
    state.select_cell(sheet1, time, 0);
    state.toggle_cell_selection(sheet1, time, 2);
    let stacked = state.copy_selection(sheet1).expect("same column");
    assert_eq!(stacked.lines().count(), 2);

    state.select_cell(sheet1, time, 0);
    state.toggle_cell_selection(sheet1, signal, 2);
    assert!(!state.grid_menu(sheet1).unwrap().copy_enabled);
    assert_eq!(state.copy_selection(sheet1), None);
}

/// Clearing non-contiguous rows empties each; Delete does not shift (holes
/// would make the shift-up ambiguous) and just empties.
#[test]
fn clearing_non_contiguous_rows_empties_each_without_shifting() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let time = column_id(&state, sheet1, "Time");
    let rows = state.row_count(sheet1);
    assert!(rows >= 4);
    let kept = cell(&state, time, 1);

    state.select_row(sheet1, 0);
    state.toggle_row_selection(sheet1, 2);
    assert!(state.is_row_selected(sheet1, 0));
    assert!(!state.is_row_selected(sheet1, 1));
    assert!(state.is_row_selected(sheet1, 2));
    assert!(!state.grid_menu(sheet1).unwrap().offers_delete_shift);

    state.dispatch(Action::DeleteSelectedCells { sheet: sheet1 });

    assert_eq!(state.row_count(sheet1), rows, "nothing moved up");
    assert_eq!(cell(&state, time, 1), kept, "row 1 is intact");
    assert_ne!(cell(&state, time, 0), kept);
    state.dispatch(Action::Undo);
}

/// A Ctrl+click that removes the last row or column does not leave an
/// empty selection.
#[test]
fn toggling_off_the_last_item_clears_the_selection() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let time = column_id(&state, sheet1, "Time");

    state.select_column(sheet1, time);
    state.toggle_column_selection(sheet1, time);
    assert!(state.selection(sheet1).is_none());

    state.select_row(sheet1, 1);
    state.toggle_row_selection(sheet1, 1);
    assert!(state.selection(sheet1).is_none());
}

/// Type and role of several columns form **a single** undo step
/// (#159); on a single column, no group.
#[test]
fn changing_type_and_role_of_several_columns_is_one_undo_step() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    let sheet1 = sheet_named(state.tree(), "Sheet1");
    let time = column_id(&state, sheet1, "Time");
    let signal = column_id(&state, sheet1, "Signal");
    let role_before = (state.column_meta(time).role, state.column_meta(signal).role);
    let type_before = (
        state
            .tree()
            .node(time)
            .unwrap()
            .as_column()
            .unwrap()
            .column_type(),
        state
            .tree()
            .node(signal)
            .unwrap()
            .as_column()
            .unwrap()
            .column_type(),
    );

    state.dispatch(Action::SetColumnsRole {
        columns: vec![time, signal],
        role: ColumnRole::Z,
    });
    assert_eq!(state.column_meta(time).role, ColumnRole::Z);
    assert_eq!(state.column_meta(signal).role, ColumnRole::Z);

    state.dispatch(Action::Undo);
    assert_eq!(
        (state.column_meta(time).role, state.column_meta(signal).role),
        role_before,
        "a single Undo gives back both roles"
    );

    state.dispatch(Action::SetColumnsType {
        columns: vec![time, signal],
        column_type: ColumnType::Mixed,
    });
    state.dispatch(Action::Undo);
    let type_after = (
        state
            .tree()
            .node(time)
            .unwrap()
            .as_column()
            .unwrap()
            .column_type(),
        state
            .tree()
            .node(signal)
            .unwrap()
            .as_column()
            .unwrap()
            .column_type(),
    );
    assert_eq!(
        type_after, type_before,
        "a single Undo gives back both types"
    );
}

/// S-F8 (#228): the geometry of a closed dialog is written to the preferences,
/// outside the history and outside the project, and read back at the next session.
#[test]
fn the_geometry_of_a_closed_dialog_goes_to_the_preferences() {
    use stokesforge_app_state::preferences::{DialogGeometry, Preferences};
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("preferences.json");
    let session = Session::spawn(Project::new("Vide").unwrap(), None).unwrap();
    let mut state = AppState::new_with_preferences(
        session.handle(),
        dir.path().join("projet.stf").display().to_string(),
        Preferences::load_from(Some(path.clone())),
    )
    .unwrap();
    let revision = state.revision();
    let geometry = DialogGeometry {
        x: 12.0,
        y: 34.0,
        width: 560.0,
        height: 420.0,
    };
    state.dispatch(Action::RememberDialogGeometry {
        kind: "import".to_string(),
        geometry,
    });
    assert_eq!(state.dialog_geometries().get("import"), Some(&geometry));
    assert_eq!(
        Preferences::load_from(Some(path))
            .dialog_geometries()
            .get("import"),
        Some(&geometry)
    );
    assert_eq!(state.revision(), revision, "neither command nor history");
    assert!(!state.can_undo());
    session.shutdown().unwrap();
}

/// S-M7 (#228): outside any dialog, an error stays in the status bar;
/// an open dialog takes it in its footer, and it does not outlive it.
#[test]
fn an_error_goes_to_the_footer_of_the_open_dialog_only() {
    let _isolation = isolate_recipe_store();
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = start_demo(dir.path());
    state.open_import(None::<NodeId>);
    state.import_state_mut().unwrap().save_as_name = String::new();
    state.save_import_recipe();
    assert!(state.dialog_error().is_some());
    assert!(!state.status().is_some_and(|status| status.is_error()));
    state.cancel_import();
    assert!(
        state.dialog_error().is_none(),
        "the error does not outlive it"
    );

    // Witness: without a dialog, an error (preferences impossible to write:
    // a folder in place of the file) goes to the status bar.
    use stokesforge_app_state::preferences::Preferences;
    let session = Session::spawn(Project::new("Vide").unwrap(), None).unwrap();
    let mut state = AppState::new_with_preferences(
        session.handle(),
        dir.path().join("projet.stf").display().to_string(),
        Preferences::load_from(Some(dir.path().to_path_buf())),
    )
    .unwrap();
    state.dispatch(Action::SetSvgTextMode(
        stokesforge_render_svg::SvgTextMode::Outlines,
    ));
    assert!(state.dialog_error().is_none());
    assert!(
        state.status().is_some_and(|status| status.is_error()),
        "outside a dialog, the status bar"
    );
    session.shutdown().unwrap();
}

// -- Status bar: duration, history, title (#230) --

#[test]
fn an_information_clears_after_eight_seconds_a_warning_does_not() {
    let dir = tempfile::tempdir().unwrap();
    let (_session, mut state) = start_demo(dir.path());
    state.dispatch(Action::Save);
    let at = state.status().expect("saved").at;
    assert_eq!(state.status().unwrap().level, StatusLevel::Info);

    let before = at + INFO_LIFETIME - std::time::Duration::from_secs(3);
    assert_eq!(
        state.expire_status(before),
        Some(std::time::Duration::from_secs(3)),
        "the next deadline, to schedule only this repaint"
    );
    assert!(state.status().is_some());
    assert_eq!(state.expire_status(at + INFO_LIFETIME), None);
    assert!(state.status().is_none(), "the information was cleared");

    // A warning stays, whatever its age, and schedules nothing.
    state.dispatch(Action::Undo);
    state.dispatch(Action::Undo);
    let at = state.status().expect("warning").at;
    assert_eq!(state.status().unwrap().level, StatusLevel::Warning);
    assert_eq!(state.expire_status(at + INFO_LIFETIME * 10), None);
    assert!(state.status().is_some());
}

#[test]
fn the_history_keeps_the_last_fifty_messages_in_order() {
    let dir = tempfile::tempdir().unwrap();
    let (_session, mut state) = start_demo(dir.path());
    assert_eq!(state.status_history().count(), 0);
    for _ in 0..STATUS_HISTORY_LEN + 10 {
        state.dispatch(Action::Undo);
    }
    assert_eq!(state.status_history().count(), STATUS_HISTORY_LEN);
    let times: Vec<_> = state.status_history().map(|m| m.wall).collect();
    assert!(times.windows(2).all(|pair| pair[0] <= pair[1]));
    assert!(!state.status_history_open());
    state.dispatch(Action::OpenStatusHistory);
    assert!(state.status_history_open());
    state.dispatch(Action::CloseStatusHistory);
    assert!(!state.status_history_open());
}

#[test]
fn the_title_carries_the_project_name_and_a_dot_while_it_is_not_saved() {
    let dir = tempfile::tempdir().unwrap();
    let (_session, mut state) = start_demo(dir.path());
    state.dispatch(Action::Save);
    let saved = state.window_title();
    assert!(saved.contains("projet.stf"), "{saved}");
    assert!(!saved.contains('•'), "{saved}");
    state.dispatch(Action::AddGraph);
    let dirty = state.window_title();
    assert!(dirty.contains("projet.stf •"), "{dirty}");
    state.dispatch(Action::Save);
    assert_eq!(state.window_title(), saved);
}

/// #309: the report of the startup read is said like that of the File
/// menu; an empty report says nothing more.
#[test]
fn mark_loaded_warns_about_the_calculations_discarded_by_the_report() {
    let dir = tempfile::tempdir().unwrap();
    let (_session, mut state) = start(demo_project(), &dir.path().join("p.stf"));
    state.mark_loaded(&stokesforge_session::LoadReport::default());
    assert!(
        state
            .status()
            .is_none_or(|s| s.level != stokesforge_app_state::state::StatusLevel::Warning),
        "empty report: no warning"
    );
    let report = stokesforge_session::LoadReport {
        discarded_calculations: vec![stokesforge_session::DiscardedCalculation {
            id: 42,
            kind: Some("fit".into()),
        }],
        ..Default::default()
    };
    state.mark_loaded(&report);
    let status = state.status().expect("a warning");
    assert_eq!(
        status.level,
        stokesforge_app_state::state::StatusLevel::Warning
    );
    assert!(status.text.contains("#42"), "{}", status.text);
}

fn discarded(ids: &[u64]) -> Vec<stokesforge_session::DiscardedCalculation> {
    ids.iter()
        .map(|&id| stokesforge_session::DiscardedCalculation { id, kind: None })
        .collect()
}

/// #309 (F1): correction records alone are said (counted message);
/// calculations and records form **a single** message that counts both.
#[test]
fn the_report_counts_the_records_and_the_calculations_in_a_single_message() {
    let dir = tempfile::tempdir().unwrap();
    let (_session, mut state) = start(demo_project(), &dir.path().join("p.stf"));
    state.mark_loaded(&stokesforge_session::LoadReport {
        discarded_corrections: discarded(&[7]),
        ..Default::default()
    });
    let status = state.status().expect("a warning");
    assert_eq!(
        status.level,
        stokesforge_app_state::state::StatusLevel::Warning
    );
    assert_eq!(
        status.text,
        "1 correction record was discarded from the project (#7)"
    );
    state.mark_loaded(&stokesforge_session::LoadReport {
        discarded_calculations: discarded(&[3, 4]),
        discarded_corrections: discarded(&[7]),
    });
    assert_eq!(
        state.status().unwrap().text,
        "2 calculations and 1 correction record were discarded from the project (#3, #4, #7)"
    );
}

/// A `.stf` with a correction calculation that the read will discard (corrupted
/// bytes), at `path`.
fn write_file_with_a_discarded_calculation(path: &Path) {
    use stokesforge_analysis::correct::CorrectOp;
    use stokesforge_analysis::ops::Operation;
    use stokesforge_core::{CalculationDraft, OutputDraft, SeriesRef, SeriesResult};
    let mut project = Project::new("P").unwrap();
    let book = project.add_workbook(project.root(), None).unwrap();
    let sheet = project.tree().children(book).unwrap()[0];
    let y = project
        .add_column(sheet, None, Some("Y"), Column::from_f64(&[1.0, 2.0]))
        .unwrap();
    project
        .add_calculation(CalculationDraft {
            operation: Operation::Correct {
                op: CorrectOp::Subtract,
            },
            series: vec![SeriesRef {
                x: None,
                y,
                sigma: None,
            }],
            aux: None,
            outputs: vec![OutputDraft {
                series: 0,
                key: "corrected".into(),
                index: None,
                meta: ColumnMeta::default(),
                data: vec![0.0, 1.0],
            }],
            results: vec![SeriesResult::default()],
            keep_results: true,
            auto_recalc: false,
            low_r2_dialog: false,
        })
        .unwrap();
    stokesforge_persistence::save(&project, path).unwrap();
    let mut bytes = std::fs::read(path).unwrap();
    let at = bytes
        .windows(8)
        .position(|w| w == b"subtract")
        .expect("the operation is in the file");
    bytes[at..at + 8].copy_from_slice(b"subtraXX");
    std::fs::write(path, bytes).unwrap();
}

/// A session on a corrupted file, loaded by `Action::Load`.
fn start_on_a_discarding_file(dir: &Path) -> (Session, AppState, std::path::PathBuf) {
    let file = dir.join("abime.stf");
    write_file_with_a_discarded_calculation(&file);
    let (session, mut state) = start(Project::new("Vide").unwrap(), &file);
    state.dispatch(Action::Load);
    assert!(state.alert().is_none(), "the project loads");
    (session, state, file)
}

/// #309 (F3): saving to the source file of a read that discarded
/// data asks first; accepting writes, and the next save
/// asks nothing more.
#[test]
fn saving_to_the_source_with_discards_asks_then_writes() {
    let dir = tempfile::tempdir().unwrap();
    let (session, mut state, file) = start_on_a_discarding_file(dir.path());
    let before = std::fs::read(&file).unwrap();

    state.dispatch(Action::Save);
    let (name, what) = state.pending_discarded_save().expect("a question");
    assert_eq!(name, "abime.stf");
    assert_eq!(what, "1 calculation");
    assert!(state.blocking_dialog());
    assert!(state.alert().is_none(), "waiting is not a failure");
    assert_eq!(std::fs::read(&file).unwrap(), before, "nothing written");

    state.dispatch(Action::ConfirmDiscardedSave(true));
    assert!(state.pending_discarded_save().is_none());
    assert_ne!(std::fs::read(&file).unwrap(), before, "written");
    let (_, _, report) = stokesforge_persistence::load_with_report(&file).unwrap();
    assert!(report.is_clean(), "the rewritten file is clean");

    let written = std::fs::read(&file).unwrap();
    state.dispatch(Action::AddGraph);
    state.dispatch(Action::Save);
    assert!(state.pending_discarded_save().is_none(), "no more question");
    assert_ne!(std::fs::read(&file).unwrap(), written);
    session.shutdown().unwrap();
}

/// Cancel writes nothing and forgets the gesture that was waiting for the save.
#[test]
fn refusing_the_overwrite_writes_nothing_and_forgets_the_gesture() {
    let dir = tempfile::tempdir().unwrap();
    let (session, mut state, file) = start_on_a_discarding_file(dir.path());
    let before = std::fs::read(&file).unwrap();
    state.dispatch(Action::AddGraph);
    state.dispatch(Action::Quit {
        pending_edits: false,
    });
    state.dispatch(Action::ConfirmUnsaved(UnsavedChoice::Save));
    assert!(state.pending_discarded_save().is_some());
    assert!(!state.take_quit_ready());

    state.dispatch(Action::ConfirmDiscardedSave(false));
    assert!(state.pending_discarded_save().is_none());
    assert_eq!(std::fs::read(&file).unwrap(), before, "nothing written");
    assert!(!state.take_quit_ready(), "the gesture is forgotten");

    // The question comes back, and accepting does not run the forgotten Quit.
    state.dispatch(Action::Save);
    assert!(
        state.pending_discarded_save().is_some(),
        "discards still there"
    );
    state.dispatch(Action::ConfirmDiscardedSave(true));
    assert!(!state.take_quit_ready(), "Quit forgotten");
    session.shutdown().unwrap();
}

/// Quit, Save, then accept: the project is saved, then the application quits.
#[test]
fn quit_save_accept_saves_then_quits() {
    let dir = tempfile::tempdir().unwrap();
    let (session, mut state, file) = start_on_a_discarding_file(dir.path());
    let before = std::fs::read(&file).unwrap();
    state.dispatch(Action::AddGraph);
    state.dispatch(Action::Quit {
        pending_edits: false,
    });
    state.dispatch(Action::ConfirmUnsaved(UnsavedChoice::Save));
    assert!(!state.take_quit_ready(), "waits for the answer");
    state.dispatch(Action::ConfirmDiscardedSave(true));
    assert_ne!(std::fs::read(&file).unwrap(), before);
    assert!(state.take_quit_ready(), "saved, then Quit");
    session.shutdown().unwrap();
}

/// Save As to ANOTHER path asks nothing and leaves the source
/// intact; going back to it asks as long as the discards are not saved, and
/// Cancel restores the previous path.
#[test]
fn save_as_elsewhere_asks_nothing_but_going_back_to_the_source_does() {
    let dir = tempfile::tempdir().unwrap();
    let (session, mut state, file) = start_on_a_discarding_file(dir.path());
    let before = std::fs::read(&file).unwrap();
    let other = dir.path().join("autre.stf");

    state.dispatch(Action::SaveAs {
        path: other.display().to_string(),
    });
    assert!(
        state.pending_discarded_save().is_none(),
        "elsewhere: no question"
    );
    assert!(other.exists());
    assert_eq!(std::fs::read(&file).unwrap(), before, "source intact");
    assert_eq!(state.path(), other.display().to_string());

    state.dispatch(Action::SaveAs {
        path: file.display().to_string(),
    });
    assert!(
        state.pending_discarded_save().is_some(),
        "back to the source"
    );
    state.dispatch(Action::ConfirmDiscardedSave(false));
    assert_eq!(state.path(), other.display().to_string(), "path restored");
    assert_eq!(std::fs::read(&file).unwrap(), before);

    state.dispatch(Action::SaveAs {
        path: file.display().to_string(),
    });
    state.dispatch(Action::ConfirmDiscardedSave(true));
    assert_eq!(state.path(), file.display().to_string());
    assert_ne!(std::fs::read(&file).unwrap(), before);
    session.shutdown().unwrap();
}

/// A clean file, or a new project, opens no question.
#[test]
fn a_clean_file_or_a_new_project_opens_no_question() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("propre.stf");
    stokesforge_persistence::save(&demo_project(), &file).unwrap();
    let (session, mut state) = start(Project::new("Vide").unwrap(), &file);
    state.dispatch(Action::Load);
    state.dispatch(Action::Save);
    assert!(state.pending_discarded_save().is_none());

    // A new project clears the mark of the corrupted source.
    let (session2, mut state2, bad) = start_on_a_discarding_file(dir.path());
    state2.dispatch(Action::NewProject {
        pending_edits: false,
    });
    state2.set_path(bad.display().to_string());
    state2.dispatch(Action::Save);
    assert!(state2.pending_discarded_save().is_none());
    session.shutdown().unwrap();
    session2.shutdown().unwrap();
}

/// The startup load (`mark_loaded`) sets the same mark as the File menu:
/// the first Save on the source asks.
#[test]
fn the_startup_load_sets_the_source_mark() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("p.stf");
    let (session, mut state) = start(demo_project(), &file);
    state.mark_loaded(&stokesforge_session::LoadReport::default());
    state.dispatch(Action::Save);
    assert!(state.pending_discarded_save().is_none(), "clean report");

    state.mark_loaded(&stokesforge_session::LoadReport {
        discarded_corrections: discarded(&[7]),
        ..Default::default()
    });
    let before = std::fs::read(&file).unwrap();
    state.dispatch(Action::Save);
    let (_, what) = state.pending_discarded_save().expect("a question");
    assert_eq!(what, "1 correction record");
    assert_eq!(std::fs::read(&file).unwrap(), before);
    session.shutdown().unwrap();
}

// -- Autosave and recovery (#317) --

mod autosave {
    use super::*;
    use std::path::PathBuf;
    use std::time::{Duration, Instant};

    const PERIOD: Duration = Duration::from_secs(300);

    /// Session on the demo, rescue folder in `dir/recovery`.
    fn start_recovery(dir: &Path, project_file: &str) -> (Session, AppState, PathBuf) {
        let path = if project_file.is_empty() {
            Path::new("").to_path_buf()
        } else {
            dir.join(project_file)
        };
        let (session, mut state) = start(demo_project(), &path);
        let recovery = dir.join("recovery");
        state.set_recovery_dir(Some(recovery.clone()));
        (session, state, recovery)
    }

    /// The `.stf` files of the rescue folder (the record and the lock do not
    /// count).
    fn copies(recovery: &Path) -> Vec<PathBuf> {
        let Ok(entries) = std::fs::read_dir(recovery) else {
            return Vec::new();
        };
        let mut copies: Vec<PathBuf> = entries
            .filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|x| x == "stf"))
            .collect();
        copies.sort();
        copies
    }

    fn change(state: &mut AppState) {
        state.dispatch(Action::AddGraph);
    }

    // -- Preferences, via the actions --

    #[test]
    fn autosave_is_on_by_default_with_five_minutes() {
        let dir = tempfile::tempdir().unwrap();
        let (_session, mut state, _) = start_recovery(dir.path(), "a.stf");
        assert!(state.autosave_enabled());
        assert_eq!(state.autosave_minutes(), 5);
        state.dispatch(Action::SetAutosaveMinutes(999));
        assert_eq!(state.autosave_minutes(), 60);
        state.dispatch(Action::SetAutosaveMinutes(0));
        assert_eq!(state.autosave_minutes(), 1);
        state.dispatch(Action::SetAutosave(false));
        assert!(!state.autosave_enabled());
    }

    // -- The write --

    /// A change then the deadline: a copy that reads back equal to the model, with
    /// no trace in the history, the title, the path, nor the Quit
    /// question; the user's `.stf` is not written.
    #[test]
    fn the_copy_reads_back_the_model_without_touching_history_title_or_quit() {
        let dir = tempfile::tempdir().unwrap();
        let (_session, mut state, recovery) = start_recovery(dir.path(), "a.stf");
        let t0 = Instant::now();
        assert_eq!(state.poll_autosave(t0), Some(PERIOD));
        change(&mut state);
        let undo_label = state.snapshot().history.undo_label.clone();
        let revision = state.snapshot().revision;
        let title = state.window_title();
        assert!(undo_label.is_some());

        state.poll_autosave(t0 + PERIOD);

        let copies = copies(&recovery);
        assert_eq!(copies.len(), 1);
        let name = copies[0]
            .file_name()
            .unwrap()
            .to_string_lossy()
            .into_owned();
        assert!(name.starts_with("a-"), "project name: {name}");
        let back = stokesforge_persistence::load(&copies[0]).expect("the copy reads back");
        assert_eq!(back.tree(), state.tree());
        assert!(
            !dir.path().join("a.stf").exists(),
            "the .stf is not written"
        );
        assert_eq!(state.snapshot().history.undo_label, undo_label);
        assert_eq!(state.snapshot().revision, revision);
        assert_eq!(state.window_title(), title, "'modified' title unchanged");
        assert_eq!(state.path(), dir.path().join("a.stf").display().to_string());

        state.dispatch(Action::Quit {
            pending_edits: false,
        });
        assert_eq!(
            state.unsaved_pending(),
            Some(&UnsavedGesture::Quit),
            "the copy did not save the project"
        );
        state.dispatch(Action::ConfirmUnsaved(UnsavedChoice::Cancel));
        // Witness: a real save does raise the question.
        state.dispatch(Action::Save);
        state.dispatch(Action::Quit {
            pending_edits: false,
        });
        assert!(state.take_quit_ready(), "saved: direct Quit");
    }

    #[test]
    fn before_the_deadline_nothing_is_written() {
        let dir = tempfile::tempdir().unwrap();
        let (_session, mut state, recovery) = start_recovery(dir.path(), "a.stf");
        let t0 = Instant::now();
        state.poll_autosave(t0);
        change(&mut state);
        let almost = t0 + PERIOD - Duration::from_secs(1);
        assert_eq!(state.poll_autosave(almost), Some(Duration::from_secs(1)));
        assert!(copies(&recovery).is_empty());
        state.poll_autosave(t0 + PERIOD);
        assert_eq!(copies(&recovery).len(), 1);
    }

    /// Nothing changed since startup, then since the last copy:
    /// no write (the copy deleted by hand does not come back).
    #[test]
    fn without_a_change_no_write() {
        let dir = tempfile::tempdir().unwrap();
        let (_session, mut state, recovery) = start_recovery(dir.path(), "a.stf");
        let t0 = Instant::now();
        state.poll_autosave(t0);
        state.poll_autosave(t0 + PERIOD);
        assert!(copies(&recovery).is_empty(), "nothing since startup");

        change(&mut state);
        state.poll_autosave(t0 + 2 * PERIOD);
        let copy = copies(&recovery).pop().expect("copy written");
        std::fs::remove_file(&copy).unwrap();
        state.poll_autosave(t0 + 3 * PERIOD);
        assert!(copies(&recovery).is_empty(), "nothing since the last copy");
    }

    /// A save resets the mark: no copy as long as nothing changes.
    #[test]
    fn after_a_save_nothing_to_copy() {
        let dir = tempfile::tempdir().unwrap();
        let (_session, mut state, recovery) = start_recovery(dir.path(), "a.stf");
        let t0 = Instant::now();
        state.poll_autosave(t0);
        change(&mut state);
        state.dispatch(Action::Save);
        state.poll_autosave(t0 + PERIOD);
        assert!(copies(&recovery).is_empty());
    }

    /// Two deadlines with changes: a single file, overwritten.
    #[test]
    fn two_deadlines_overwrite_the_same_copy() {
        let dir = tempfile::tempdir().unwrap();
        let (_session, mut state, recovery) = start_recovery(dir.path(), "a.stf");
        let t0 = Instant::now();
        state.poll_autosave(t0);
        change(&mut state);
        state.poll_autosave(t0 + PERIOD);
        change(&mut state);
        state.poll_autosave(t0 + 2 * PERIOD);
        let copies = copies(&recovery);
        assert_eq!(copies.len(), 1, "{copies:?}");
        let back = stokesforge_persistence::load(&copies[0]).unwrap();
        assert_eq!(back.tree(), state.tree(), "the second write wins");
    }

    #[test]
    fn preference_off_nothing_is_written() {
        let dir = tempfile::tempdir().unwrap();
        let (_session, mut state, recovery) = start_recovery(dir.path(), "a.stf");
        state.dispatch(Action::SetAutosave(false));
        let t0 = Instant::now();
        assert_eq!(state.poll_autosave(t0), None);
        change(&mut state);
        assert_eq!(state.poll_autosave(t0 + 10 * PERIOD), None);
        assert!(copies(&recovery).is_empty());
    }

    /// The set period applies, from the setting.
    #[test]
    fn the_set_period_is_the_timer_period() {
        let dir = tempfile::tempdir().unwrap();
        let (_session, mut state, recovery) = start_recovery(dir.path(), "a.stf");
        let t0 = Instant::now();
        state.poll_autosave(t0);
        // Set along the way: the new period counts from the next frame,
        // not at the old deadline (5 minutes).
        state.dispatch(Action::SetAutosaveMinutes(1));
        change(&mut state);
        let t1 = t0 + Duration::from_secs(10);
        state.poll_autosave(t1);
        state.poll_autosave(t1 + Duration::from_secs(59));
        assert!(copies(&recovery).is_empty());
        state.poll_autosave(t1 + Duration::from_secs(60));
        assert_eq!(copies(&recovery).len(), 1);
    }

    #[test]
    fn a_project_without_a_path_is_copied_as_untitled() {
        let dir = tempfile::tempdir().unwrap();
        let (_session, mut state, recovery) = start_recovery(dir.path(), "");
        let t0 = Instant::now();
        state.poll_autosave(t0);
        change(&mut state);
        state.poll_autosave(t0 + PERIOD);
        let copies = copies(&recovery);
        assert_eq!(copies.len(), 1);
        let name = copies[0]
            .file_name()
            .unwrap()
            .to_string_lossy()
            .into_owned();
        assert!(name.starts_with("Untitled-"), "{name}");
    }

    /// Open macro: the deadline is pushed back without error; closed, the copy
    /// goes out at the new attempt.
    #[test]
    fn during_a_macro_the_deadline_is_skipped_without_error() {
        let dir = tempfile::tempdir().unwrap();
        let (session, mut state, recovery) = start_recovery(dir.path(), "a.stf");
        let t0 = Instant::now();
        state.poll_autosave(t0);
        change(&mut state);
        let handle = session.handle();
        handle.begin_macro(MacroKind::Import).unwrap();
        state.refresh();
        assert!(state.snapshot().history.in_macro);
        let due = t0 + PERIOD;
        let messages = state.status_history().count();
        let retry = state.poll_autosave(due).expect("re-armed");
        assert!(retry < PERIOD, "closer new attempt: {retry:?}");
        assert!(copies(&recovery).is_empty());
        assert_eq!(
            state.status_history().count(),
            messages,
            "no message: no warning or error"
        );
        handle.end_macro().unwrap();
        state.refresh();
        state.poll_autosave(due + retry);
        assert_eq!(copies(&recovery).len(), 1);
    }

    /// A recalculation running: same skip.
    #[test]
    fn during_a_recalculation_the_deadline_is_skipped() {
        let dir = tempfile::tempdir().unwrap();
        let (_session, mut state) = start(heavy_formula_project(), &dir.path().join("l.stf"));
        let recovery = dir.path().join("recovery");
        state.set_recovery_dir(Some(recovery.clone()));
        let t0 = Instant::now();
        state.poll_autosave(t0);
        state.dispatch(Action::AddGraph);
        assert!(state.recalc_work().is_some(), "recalculation running");
        let retry = state.poll_autosave(t0 + PERIOD).expect("re-armed");
        assert!(retry < PERIOD);
        assert!(copies(&recovery).is_empty());
        finish_recalculation(&mut state);
        state.poll_autosave(t0 + PERIOD + retry);
        assert_eq!(
            copies(&recovery).len(),
            1,
            "messages: {}",
            status_messages(&state)
        );
    }

    /// Impossible folder: a single warning as long as it lasts.
    #[test]
    fn an_impossible_folder_says_only_a_warning() {
        let dir = tempfile::tempdir().unwrap();
        let (_session, mut state, _) = start_recovery(dir.path(), "a.stf");
        let obstacle = dir.path().join("fichier");
        std::fs::write(&obstacle, b"x").unwrap();
        state.set_recovery_dir(Some(obstacle.join("recovery")));
        let t0 = Instant::now();
        state.poll_autosave(t0);
        change(&mut state);
        let warnings = |state: &AppState| {
            state
                .status_history()
                .filter(|m| m.level == StatusLevel::Warning)
                .count()
        };
        let before = warnings(&state);
        state.poll_autosave(t0 + PERIOD);
        state.poll_autosave(t0 + 2 * PERIOD);
        state.poll_autosave(t0 + 3 * PERIOD);
        assert_eq!(warnings(&state), before + 1, "a single warning");
        assert!(state.status().is_some_and(|s| s.text
            == stokesforge_app_state::tr!(
                "status-autosave-failed",
                cause = stokesforge_app_state::errors::io::error_message(
                    &std::fs::create_dir_all(obstacle.join("recovery")).unwrap_err()
                )
            )));
    }

    // -- Deleting the copy --

    fn with_copy(dir: &Path, file: &str) -> (Session, AppState, PathBuf, Instant) {
        let (session, mut state, recovery) = start_recovery(dir, file);
        let t0 = Instant::now();
        state.poll_autosave(t0);
        change(&mut state);
        state.poll_autosave(t0 + PERIOD);
        assert_eq!(copies(&recovery).len(), 1, "copy written");
        (session, state, recovery, t0)
    }

    #[test]
    fn saving_deletes_the_copy() {
        let dir = tempfile::tempdir().unwrap();
        let (_s, mut state, recovery, _) = with_copy(dir.path(), "a.stf");
        state.dispatch(Action::Save);
        assert!(copies(&recovery).is_empty());
        assert_eq!(
            std::fs::read_dir(&recovery).unwrap().count(),
            0,
            "record and lock too"
        );
    }

    #[test]
    fn save_as_deletes_the_copy() {
        let dir = tempfile::tempdir().unwrap();
        let (_s, mut state, recovery, _) = with_copy(dir.path(), "");
        state.dispatch(Action::SaveAs {
            path: dir.path().join("neuf.stf").display().to_string(),
        });
        assert!(copies(&recovery).is_empty());
    }

    #[test]
    fn closing_by_discarding_deletes_the_copy() {
        let dir = tempfile::tempdir().unwrap();
        let (_s, mut state, recovery, _) = with_copy(dir.path(), "a.stf");
        state.dispatch(Action::Quit {
            pending_edits: false,
        });
        assert_eq!(
            copies(&recovery).len(),
            1,
            "the question is asked, nothing is lost"
        );
        state.dispatch(Action::ConfirmUnsaved(UnsavedChoice::Discard));
        assert!(state.take_quit_ready());
        assert!(copies(&recovery).is_empty());
    }

    #[test]
    fn new_project_deletes_the_copy() {
        let dir = tempfile::tempdir().unwrap();
        let (_s, mut state, recovery, _) = with_copy(dir.path(), "a.stf");
        state.dispatch(Action::NewProject {
            pending_edits: false,
        });
        state.dispatch(Action::ConfirmUnsaved(UnsavedChoice::Discard));
        assert!(copies(&recovery).is_empty());
    }

    #[test]
    fn opening_another_project_deletes_the_copy() {
        let dir = tempfile::tempdir().unwrap();
        let other = dir.path().join("autre.stf");
        {
            let (_s, mut state) = start_demo(dir.path());
            state.dispatch(Action::SaveAs {
                path: other.display().to_string(),
            });
        }
        let (_s, mut state, recovery, _) = with_copy(dir.path(), "a.stf");
        state.dispatch(Action::OpenProject {
            path: other.display().to_string(),
            pending_edits: false,
        });
        state.dispatch(Action::ConfirmUnsaved(UnsavedChoice::Discard));
        assert!(copies(&recovery).is_empty());
    }

    /// Canceling the question keeps the copy.
    #[test]
    fn canceling_the_question_keeps_the_copy() {
        let dir = tempfile::tempdir().unwrap();
        let (_s, mut state, recovery, _) = with_copy(dir.path(), "a.stf");
        state.dispatch(Action::Quit {
            pending_edits: false,
        });
        state.dispatch(Action::ConfirmUnsaved(UnsavedChoice::Cancel));
        assert_eq!(copies(&recovery).len(), 1);
    }

    /// Lost session then Close: the automatic copy is kept.
    #[test]
    fn lost_session_then_close_keeps_the_copy() {
        let dir = tempfile::tempdir().unwrap();
        let (session, mut state, recovery, _) = with_copy(dir.path(), "a.stf");
        let automatic = copies(&recovery);
        session.shutdown().unwrap();
        state.poll_recalc();
        assert!(state.session_lost());
        state.dispatch(Action::DismissAlert);
        assert!(state.take_quit_ready(), "Close closes the application");
        for copy in automatic {
            assert!(copy.exists(), "the automatic copy stays: {copy:?}");
        }
    }

    // -- Recovery at startup --

    /// A copy from a vanished process: written without a lock, like #310.
    fn dead_copy(recovery: &Path, project_path: &str, graphs: usize) -> (PathBuf, Tree) {
        let dir = tempfile::tempdir().unwrap();
        let (_session, mut source) = start_demo(dir.path());
        for _ in 0..graphs {
            source.dispatch(Action::AddGraph);
        }
        let path =
            stokesforge_app_state::recovery::write_copy(recovery, project_path, source.snapshot())
                .unwrap();
        (path, source.tree().as_ref().clone())
    }

    #[test]
    fn open_takes_the_original_path_and_an_empty_history() {
        let dir = tempfile::tempdir().unwrap();
        let recovery = dir.path().join("recovery");
        let origin = dir.path().join("travail.stf");
        let (copy, tree) = dead_copy(&recovery, &origin.display().to_string(), 2);
        // An empty project: nothing to lose, Open loads without asking a question.
        let (_s, mut state) = start(Project::new("Empty").unwrap(), Path::new(""));
        state.set_recovery_dir(Some(recovery.clone()));
        state.scan_recovery();
        let rows = state.recovery_rows().expect("a proposed copy");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].path, copy);

        state.dispatch(Action::RecoveryOpen(copy.clone()));

        assert_eq!(state.tree().as_ref(), &tree);
        assert_eq!(state.path(), origin.display().to_string());
        assert!(!state.snapshot().history.can_undo, "empty history");
        assert!(state.recovery_rows().is_none(), "empty list: window closed");
        assert!(!copy.exists(), "the opened copy is deleted");
        assert_eq!(copies(&recovery).len(), 1, "copied again into the location");
        // Unsaved work: closing asks the question.
        state.dispatch(Action::Quit {
            pending_edits: false,
        });
        assert_eq!(state.unsaved_pending(), Some(&UnsavedGesture::Quit));
    }

    #[test]
    fn open_of_a_copy_without_a_path_leaves_the_path_empty() {
        let dir = tempfile::tempdir().unwrap();
        let recovery = dir.path().join("recovery");
        let (copy, _) = dead_copy(&recovery, "", 1);
        let (_s, mut state) = start(Project::new("Vide").unwrap(), &dir.path().join("x.stf"));
        state.set_recovery_dir(Some(recovery));
        state.scan_recovery();
        state.dispatch(Action::RecoveryOpen(copy));
        assert_eq!(state.path(), "");
    }

    /// Work in progress: the "Save?" question first; the list
    /// stays open for the other copies.
    #[test]
    fn open_asks_to_save_and_the_list_stays_open() {
        let dir = tempfile::tempdir().unwrap();
        let recovery = dir.path().join("recovery");
        let (first, _) = dead_copy(&recovery, "", 1);
        let (second, _) = dead_copy(&recovery, "/x/b.stf", 2);
        let (_s, mut state) = start_demo(dir.path());
        change(&mut state);
        state.set_recovery_dir(Some(recovery.clone()));
        state.scan_recovery();
        assert_eq!(state.recovery_rows().unwrap().len(), 2);

        state.dispatch(Action::RecoveryOpen(first.clone()));
        assert_eq!(
            state.unsaved_pending(),
            Some(&UnsavedGesture::OpenRecovery(first.clone()))
        );
        assert!(first.exists(), "nothing is loaded before the answer");
        state.dispatch(Action::ConfirmUnsaved(UnsavedChoice::Discard));

        assert!(!first.exists());
        let rows = state.recovery_rows().expect("the list stays open");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].path, second);
    }

    #[test]
    fn ignore_keeps_the_file_and_it_comes_back_at_the_next_startup() {
        let dir = tempfile::tempdir().unwrap();
        let recovery = dir.path().join("recovery");
        let (copy, _) = dead_copy(&recovery, "", 1);
        let (_s, mut state) = start_demo(dir.path());
        state.set_recovery_dir(Some(recovery.clone()));
        state.scan_recovery();
        state.dispatch(Action::RecoveryIgnore(copy.clone()));
        assert!(state.recovery_rows().is_none());
        assert!(copy.exists(), "Ignore keeps the file");
        let (_s2, mut next) = start_demo(dir.path());
        next.set_recovery_dir(Some(recovery));
        next.scan_recovery();
        assert_eq!(next.recovery_rows().unwrap().len(), 1, "offered again");
    }

    #[test]
    fn delete_asks_confirmation_then_deletes_copy_record_and_lock() {
        let dir = tempfile::tempdir().unwrap();
        let recovery = dir.path().join("recovery");
        let (copy, _) = dead_copy(&recovery, "/x/b.stf", 1);
        std::fs::write(copy.with_extension("lock"), b"").unwrap();
        let (_s, mut state) = start_demo(dir.path());
        state.set_recovery_dir(Some(recovery.clone()));
        state.scan_recovery();

        state.dispatch(Action::RecoveryDelete(copy.clone()));
        assert!(copy.exists(), "nothing is deleted before the confirmation");
        assert_eq!(state.pending_recovery_delete().as_deref(), Some("b.stf"));
        assert!(state.blocking_dialog());
        state.dispatch(Action::ConfirmRecoveryDelete(false));
        assert!(copy.exists(), "Cancel keeps the copy");
        assert!(state.recovery_rows().is_some());

        state.dispatch(Action::RecoveryDelete(copy.clone()));
        state.dispatch(Action::ConfirmRecoveryDelete(true));
        assert_eq!(std::fs::read_dir(&recovery).unwrap().count(), 0);
        assert!(state.recovery_rows().is_none());
    }

    /// Close ignores everything: the files stay.
    #[test]
    fn close_ignores_the_remaining_copies() {
        let dir = tempfile::tempdir().unwrap();
        let recovery = dir.path().join("recovery");
        let (copy, _) = dead_copy(&recovery, "", 1);
        let (_s, mut state) = start_demo(dir.path());
        state.set_recovery_dir(Some(recovery));
        state.scan_recovery();
        state.dispatch(Action::CloseRecovery);
        assert!(state.recovery_rows().is_none());
        assert!(copy.exists());
    }

    /// Without a rescue folder (tests, system with no data folder): nothing
    /// is offered, nothing is written.
    #[test]
    fn without_a_rescue_folder_nothing_happens() {
        let dir = tempfile::tempdir().unwrap();
        let (_s, mut state) = start_demo(dir.path());
        state.set_recovery_dir(None);
        state.scan_recovery();
        assert!(state.recovery_rows().is_none());
        let t0 = Instant::now();
        state.poll_autosave(t0);
        change(&mut state);
        state.poll_autosave(t0 + PERIOD);
    }
}

// -- Issue #382: Help > About StokesForge --

/// The About window opens and closes through its two actions; Copy hands the
/// version line to the shell and says so in the status bar.
#[test]
fn the_about_window_opens_closes_and_copies_the_version_line() {
    let dir = tempfile::tempdir().expect("temporary folder");
    let (_session, mut state, _graph) = demo_with_graph(dir.path());
    assert!(!state.about_open());
    state.dispatch(Action::OpenAbout);
    assert!(state.about_open());
    state.dispatch(Action::CloseAbout);
    assert!(!state.about_open());

    state.dispatch(Action::CopyVersionLine);
    let line = state.take_object_clipboard_text().expect("a line to copy");
    assert_eq!(line, stokesforge_app_state::startup::version_line());
    let status = state.status().expect("a message");
    assert!(!status.is_error());
    assert_eq!(
        status.text,
        stokesforge_app_state::tr!("status-version-copied")
    );
}

// -- Preferences window settings (#148, lot A) --

/// A state on `session` that reads and writes the preferences `file`.
fn state_on_preferences(session: &Session, dir: &Path, file: &Path) -> AppState {
    AppState::new_with_preferences(
        session.handle(),
        dir.join("projet.stf").display().to_string(),
        stokesforge_app_state::preferences::Preferences::load_from(Some(file.to_path_buf())),
    )
    .expect("state")
}

/// W3: the PNG resolution and the SVG text mode outlive the session; a value
/// outside the proposed list is ignored; a file that cannot be written reports it.
#[test]
fn the_png_resolution_and_the_svg_text_are_kept_across_states_on_the_same_file() {
    use stokesforge_render_svg::SvgTextMode;
    let dir = tempfile::tempdir().expect("temporary directory");
    let (session, _state, _sheet, _columns) = kinetics(dir.path());
    let file = dir.path().join("preferences.json");
    let mut first = state_on_preferences(&session, dir.path(), &file);
    assert_eq!(first.png_export_scale(), 4.0, "default");
    first.dispatch(Action::SetPngExportScale(2.0));
    first.dispatch(Action::SetPngExportScale(3.0));
    first.dispatch(Action::SetSvgTextMode(SvgTextMode::Outlines));
    let second = state_on_preferences(&session, dir.path(), &file);
    assert_eq!(second.png_export_scale(), 2.0);
    assert_eq!(second.svg_text_mode(), SvgTextMode::Outlines);

    // A write failure (a folder in place of the file) is reported.
    let mut broken = state_on_preferences(&session, dir.path(), dir.path());
    broken.dispatch(Action::SetPngExportScale(1.0));
    assert!(broken.status().is_some_and(|status| status.is_error()));
}

/// R9: the preference rides in the `PlotSpec` of the three creation sites
/// (AddGraph, PlotColumns, PlotRows), in the same command (one undo step); the
/// graphs that exist keep their own setting.
#[test]
fn new_graphs_take_the_decimal_comma_preference_and_existing_ones_do_not() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (session, _state, sheet, columns) = kinetics(dir.path());
    let file = dir.path().join("preferences.json");
    let mut state = state_on_preferences(&session, dir.path(), &file);
    let root = state.tree().root();
    let last_graph = |state: &AppState| {
        state
            .tree()
            .node(root)
            .and_then(|node| node.children().last().copied())
            .expect("graph")
    };
    state.dispatch(Action::AddGraph);
    let old = last_graph(&state);
    assert!(
        !state.plot_spec(old).unwrap().decimal_comma,
        "point by default"
    );

    state.dispatch(Action::SetNewGraphDecimalComma(true));
    assert!(state.new_graph_decimal_comma());
    let before = state.tree().len();
    state.dispatch(Action::AddGraph);
    assert!(state.plot_spec(last_graph(&state)).unwrap().decimal_comma);
    state.dispatch(Action::Undo);
    assert_eq!(state.tree().len(), before, "a single history step");
    state.dispatch(Action::PlotColumns {
        sheet,
        columns: columns.clone(),
        plot_type: PlotType::Line,
    });
    assert!(state.plot_spec(last_graph(&state)).unwrap().decimal_comma);
    state.dispatch(Action::PlotRows {
        sheet,
        rows: vec![0, 1],
        plot_type: PlotType::Line,
    });
    assert!(state.plot_spec(last_graph(&state)).unwrap().decimal_comma);
    assert!(
        !state.plot_spec(old).unwrap().decimal_comma,
        "witness: the existing graph is untouched"
    );
    // Remembered, and turned off again.
    let mut next = state_on_preferences(&session, dir.path(), &file);
    assert!(next.new_graph_decimal_comma());
    next.dispatch(Action::SetNewGraphDecimalComma(false));
    next.dispatch(Action::AddGraph);
    assert!(!next.plot_spec(last_graph(&next)).unwrap().decimal_comma);
}

/// W6, M5: the formula help preference is remembered and on by default.
#[test]
fn the_formula_help_and_the_analysis_default_are_remembered() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (session, _state, _sheet, _columns) = kinetics(dir.path());
    let file = dir.path().join("preferences.json");
    let mut first = state_on_preferences(&session, dir.path(), &file);
    assert!(first.show_formula_help(), "shown by default");
    assert!(first.show_column_formula_help(), "shown by default");
    first.dispatch(Action::SetShowFormulaHelp(false));
    first.dispatch(Action::SetShowColumnFormulaHelp(false));
    first.dispatch(Action::SetAnalysisKeepResults(true));
    let second = state_on_preferences(&session, dir.path(), &file);
    assert!(!second.show_formula_help());
    assert!(!second.show_column_formula_help());
    assert!(second.analysis_keep_results_default());
}

// -- Layout of the fixed panels (#148, lot B) --

/// `Action::RememberPanelLayout` writes the layout, outside the project and the
/// history; a new state on the same file finds it; a file that cannot be
/// written is reported.
#[test]
fn the_panel_layout_is_kept_across_states_and_a_failed_write_is_reported() {
    use stokesforge_app_state::panel_layout::{PanelKind, PanelLayout};
    let dir = tempfile::tempdir().expect("temporary directory");
    let (session, _state, _sheet, _columns) = kinetics(dir.path());
    let file = dir.path().join("preferences.json");
    let mut state = state_on_preferences(&session, dir.path(), &file);
    assert_eq!(state.panel_layout(), PanelLayout::default());
    let mut layout = PanelLayout {
        monitors: Some(2),
        ..PanelLayout::default()
    };
    layout.panel_mut(PanelKind::Object).detached = true;
    layout.panel_mut(PanelKind::Object).size = Some([330.0, 480.0]);
    layout.panel_mut(PanelKind::Object).position = Some([1500.0, 40.0]);
    layout.panel_mut(PanelKind::Script).collapsed = false;
    let revision = state.revision();
    let undoable = state.can_undo();
    state.dispatch(Action::RememberPanelLayout(layout));
    assert_eq!(state.revision(), revision, "neither command nor history");
    assert_eq!(state.can_undo(), undoable);
    assert_eq!(
        state_on_preferences(&session, dir.path(), &file).panel_layout(),
        layout
    );

    let mut broken = state_on_preferences(&session, dir.path(), dir.path());
    broken.dispatch(Action::RememberPanelLayout(layout));
    assert!(
        broken.status().is_some_and(|status| status.is_error()),
        "an unwritable file is reported"
    );
}

// -- Default folders of the file dialogs (#148, lot C) --

fn folder_state(dir: &Path) -> (Session, AppState) {
    let (session, _state, _sheet, _columns) = kinetics(dir);
    let file = dir.join("preferences.json");
    let state = state_on_preferences(&session, dir, &file);
    (session, state)
}

fn some(path: &str) -> Option<PathBuf> {
    Some(PathBuf::from(path))
}

/// The default serves while the session has nothing; a file dialog's answer then
/// takes over for its kind only, and a cancelled dialog changes nothing.
#[test]
fn the_default_folder_serves_first_then_the_last_folder_of_the_session() {
    use stokesforge_app_state::default_folders::FolderKind;
    use stokesforge_app_state::file_picker::PickRequest;
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = folder_state(dir.path());
    state.dispatch(Action::SetDefaultFolder {
        kind: FolderKind::Projects,
        folder: "/data/projects".to_owned(),
    });
    let open = PickRequest::OpenProject;
    let candidates = state.folder_candidates(&open);
    assert_eq!(candidates.default, some("/data/projects"));
    assert_eq!(
        candidates.session_last, None,
        "nothing used yet: the default"
    );
    assert_eq!(
        candidates.resolve(|folder| folder == Path::new("/data/projects")),
        some("/data/projects")
    );

    // A file chosen in /work: the session's folder takes over.
    state.remember_picked_folder(&open, &[PathBuf::from("/work/a.stf")]);
    let candidates = state.folder_candidates(&open);
    assert_eq!(candidates.session_last, some("/work"));
    assert_eq!(candidates.resolve(|_| true), some("/work"));
    // A missing session folder falls back to the default, silently.
    assert_eq!(
        candidates.resolve(|folder| folder == Path::new("/data/projects")),
        some("/data/projects")
    );
}

/// Save As of a project already saved starts in the file's own folder; the folder
/// it was saved in is then the Open folder; an export leaves Import alone.
#[test]
fn the_folders_follow_their_kind_and_save_as_uses_the_files_own_folder() {
    use stokesforge_app_state::default_folders::FolderKind;
    use stokesforge_app_state::file_picker::PickRequest;
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = folder_state(dir.path());
    state.set_path("/x/p.stf".to_owned());
    state.dispatch(Action::SetDefaultFolder {
        kind: FolderKind::Projects,
        folder: "/data/projects".to_owned(),
    });
    let save_as = PickRequest::SaveProjectAs {
        suggested: "p.stf".to_owned(),
    };
    let candidates = state.folder_candidates(&save_as);
    assert_eq!(candidates.session_last, some("/x"), "the file's own folder");

    // Saved as /y/q.stf: the next Open starts in /y.
    state.remember_picked_folder(&save_as, &[PathBuf::from("/y/q.stf")]);
    assert_eq!(
        state
            .folder_candidates(&PickRequest::OpenProject)
            .session_last,
        some("/y")
    );

    // An export is another kind: Import is untouched.
    let export = PickRequest::ExportGraph {
        spec: Box::new(stokesforge_plot::PlotSpec::default()),
        file_name: "g.svg".to_owned(),
    };
    state.remember_picked_folder(&export, &[PathBuf::from("/figures/g.svg")]);
    assert_eq!(
        state.folder_candidates(&export).session_last,
        some("/figures")
    );
    assert_eq!(
        state
            .folder_candidates(&PickRequest::AddImportFiles)
            .session_last,
        None,
        "witness: Import never fed"
    );
}

/// Import with no default starts where the last imported file was; the recipe
/// dialogs follow the Import kind too.
#[test]
fn the_import_dialogs_fall_back_to_the_folder_of_the_last_imported_file() {
    use stokesforge_app_state::file_picker::PickRequest;
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = folder_state(dir.path());
    assert_eq!(
        state
            .folder_candidates(&PickRequest::AddImportFiles)
            .fallback,
        None
    );
    state.open_import(None::<NodeId>);
    state
        .import_state_mut()
        .unwrap()
        .recent
        .record(&[PathBuf::from("/lab/run1/a.csv")]);
    for request in [
        PickRequest::AddImportFiles,
        PickRequest::ImportImportRecipe,
        PickRequest::ExportImportRecipe {
            file_name: "t.json".to_owned(),
        },
    ] {
        assert_eq!(
            state.folder_candidates(&request).fallback,
            some("/lab/run1"),
            "{request:?}"
        );
    }
    assert_eq!(
        state.folder_candidates(&PickRequest::OpenProject).fallback,
        None,
        "the fallback is Import's"
    );
}

/// Changing a default during the session forgets the session's folder of that
/// kind (and only that kind); the same value changes nothing.
#[test]
fn changing_a_default_forgets_the_session_memory_of_its_kind() {
    use stokesforge_app_state::default_folders::FolderKind;
    use stokesforge_app_state::file_picker::PickRequest;
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = folder_state(dir.path());
    state.remember_picked_folder(&PickRequest::OpenProject, &[PathBuf::from("/work/a.stf")]);
    state.remember_picked_folder(&PickRequest::AddImportFiles, &[PathBuf::from("/lab/a.csv")]);
    state.dispatch(Action::SetDefaultFolder {
        kind: FolderKind::Projects,
        folder: "/new".to_owned(),
    });
    let candidates = state.folder_candidates(&PickRequest::OpenProject);
    assert_eq!(candidates.session_last, None);
    assert_eq!(candidates.default, some("/new"));
    assert_eq!(
        state
            .folder_candidates(&PickRequest::AddImportFiles)
            .session_last,
        some("/lab"),
        "the other kind keeps its memory"
    );
    // Same value again: nothing forgotten.
    state.remember_picked_folder(&PickRequest::OpenProject, &[PathBuf::from("/work/b.stf")]);
    state.dispatch(Action::SetDefaultFolder {
        kind: FolderKind::Projects,
        folder: "/new".to_owned(),
    });
    assert_eq!(
        state
            .folder_candidates(&PickRequest::OpenProject)
            .session_last,
        some("/work")
    );
}

/// Choosing a default is not using one: it starts in the current default and does
/// not feed the session's memory; an empty text clears the default.
#[test]
fn choosing_a_default_starts_in_the_current_one_and_feeds_no_memory() {
    use stokesforge_app_state::default_folders::FolderKind;
    use stokesforge_app_state::file_picker::PickRequest;
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = folder_state(dir.path());
    state.dispatch(Action::SetDefaultFolder {
        kind: FolderKind::Export,
        folder: "/figs".to_owned(),
    });
    state.remember_picked_folder(
        &PickRequest::ExportGraph {
            spec: Box::new(stokesforge_plot::PlotSpec::default()),
            file_name: "g.svg".to_owned(),
        },
        &[PathBuf::from("/elsewhere/g.svg")],
    );
    let choose = PickRequest::ChooseDefaultFolder {
        kind: FolderKind::Export,
    };
    let candidates = state.folder_candidates(&choose);
    assert_eq!(candidates.session_last, None);
    assert_eq!(candidates.default, some("/figs"));
    state.remember_picked_folder(&choose, &[PathBuf::from("/chosen/folder")]);
    assert_eq!(state.default_folder(FolderKind::Export), Some("/figs"));
    state.dispatch(Action::SetDefaultFolder {
        kind: FolderKind::Export,
        folder: String::new(),
    });
    assert_eq!(state.default_folder(FolderKind::Export), None);
}

/// The defaults are text kept as typed (no `~` expansion, a Windows path
/// byte for byte) and remembered by a new state on the same file.
#[test]
fn the_default_folders_are_remembered_as_typed() {
    use stokesforge_app_state::default_folders::FolderKind;
    let dir = tempfile::tempdir().expect("temporary directory");
    let (session, mut state) = folder_state(dir.path());
    let file = dir.path().join("preferences.json");
    state.dispatch(Action::SetDefaultFolder {
        kind: FolderKind::Import,
        folder: r"C:\Data\x".to_owned(),
    });
    state.dispatch(Action::SetDefaultFolder {
        kind: FolderKind::Projects,
        folder: "~/projets".to_owned(),
    });
    let next = state_on_preferences(&session, dir.path(), &file);
    assert_eq!(next.default_folder(FolderKind::Import), Some(r"C:\Data\x"));
    assert_eq!(next.default_folder(FolderKind::Projects), Some("~/projets"));
    assert_eq!(next.default_folder(FolderKind::Export), None);
}

/// A name that is not valid UTF-8 cannot be stored: the setting is unchanged
/// and the notice says so.
#[cfg(unix)]
#[test]
fn a_folder_that_is_not_valid_text_changes_nothing_and_says_so() {
    use std::os::unix::ffi::OsStrExt;
    use stokesforge_app_state::default_folders::FolderKind;
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state) = folder_state(dir.path());
    state.dispatch(Action::SetDefaultFolder {
        kind: FolderKind::Import,
        folder: "/ok".to_owned(),
    });
    let odd = Path::new(std::ffi::OsStr::from_bytes(b"/data/\xff\xfe"));
    state.apply_default_folder_pick(FolderKind::Import, odd);
    assert_eq!(state.default_folder(FolderKind::Import), Some("/ok"));
    let status = state.status().expect("a notice");
    assert!(status.is_error());
    assert_eq!(
        status.text,
        stokesforge_app_state::tr!("error-folder-not-utf8")
    );
    // A valid name is applied.
    state.apply_default_folder_pick(FolderKind::Import, Path::new("/fine"));
    assert_eq!(state.default_folder(FolderKind::Import), Some("/fine"));
}

/// The configuration folder is the one of the preferences file; an unwritable
/// preferences file reports the default folder not saved.
#[test]
fn the_configuration_folder_is_that_of_the_preferences_file() {
    use stokesforge_app_state::default_folders::FolderKind;
    let dir = tempfile::tempdir().expect("temporary directory");
    let (session, state) = folder_state(dir.path());
    assert_eq!(state.config_folder().as_deref(), Some(dir.path()));
    let mut broken = state_on_preferences(&session, dir.path(), dir.path());
    broken.dispatch(Action::SetDefaultFolder {
        kind: FolderKind::Export,
        folder: "/f".to_owned(),
    });
    assert!(broken.status().is_some_and(|status| status.is_error()));
}

// -- Grid of cells (#357) --

/// A graph of one cell built through the real gesture, plus the X, Y, X2, Y2
/// columns of its sheet.
fn cell_bench(dir: &Path) -> (Session, AppState, NodeId, Vec<NodeId>) {
    let mut project = Project::new("Cells").expect("project");
    let root = project.root();
    let book = project.add_workbook(root, Some("Book1")).expect("workbook");
    let sheet = project
        .tree()
        .child_by_name(book, "Sheet1")
        .expect("read")
        .expect("sheet");
    let mut add = |name: &str| {
        project
            .add_column(sheet, None, Some(name), Column::from_f64(&[1.0, 2.0, 3.0]))
            .expect("column")
    };
    let c: Vec<NodeId> = ["X", "Y", "X2", "Y2"].map(&mut add).to_vec();
    let (session, mut state) = start(project, &dir.join("cells.stf"));
    for (column, role) in
        c.iter()
            .copied()
            .zip([ColumnRole::X, ColumnRole::Y, ColumnRole::X2, ColumnRole::Y2])
    {
        state.dispatch(Action::SetColumnRole { column, role });
    }
    state.dispatch(Action::PlotColumns {
        sheet,
        columns: vec![c[0], c[1]],
        plot_type: PlotType::Line,
    });
    let graph = state
        .tree()
        .node(root)
        .and_then(|node| node.children().last().copied())
        .expect("graph created");
    (session, state, graph, c)
}

/// #357: the menu gesture adds one empty cell in a single undo step.
#[test]
fn add_cell_is_one_history_entry_and_undo_restores_the_spec() {
    use stokesforge_core::compose::CellSide;
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state, graph, _c) = cell_bench(dir.path());
    let before = graph_spec(&state, graph);

    state.dispatch(Action::AddCell {
        graph,
        panel: 0,
        side: CellSide::Right,
        columns: Vec::new(),
    });
    let after = graph_spec(&state, graph);
    assert_eq!(after.panels.len(), 2);
    assert_eq!((after.panels[1].row, after.panels[1].column), (0, 1));
    assert!(after.panels[1].series.is_empty());
    assert_eq!(
        state.snapshot().history.undo_label,
        Some(HistoryLabel::EditPlotSpec(
            stokesforge_commands::PlotEditKind::AddCell
        ))
    );

    state.dispatch(Action::Undo);
    assert_eq!(graph_spec(&state, graph), before, "one Undo is enough");
    state.dispatch(Action::Redo);
    assert_eq!(graph_spec(&state, graph), after);
}

/// #357: columns dropped on the edge band go in the new cell, Y2 on its role
/// layer; without a Y column nothing is created.
#[test]
fn add_cell_with_columns_puts_the_series_in_the_new_cell_and_refuses_without_y() {
    use stokesforge_core::compose::CellSide;
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state, graph, c) = cell_bench(dir.path());
    let before = graph_spec(&state, graph);

    state.dispatch(Action::AddCell {
        graph,
        panel: 0,
        side: CellSide::Below,
        columns: vec![c[0]],
    });
    assert_eq!(graph_spec(&state, graph), before, "X only: nothing created");
    assert_eq!(
        state.status().expect("a message").text,
        stokesforge_app_state::tr!("error-add-no-y")
    );

    state.dispatch(Action::AddCell {
        graph,
        panel: 0,
        side: CellSide::Below,
        columns: c.clone(),
    });
    let after = graph_spec(&state, graph);
    assert_eq!(after.panels.len(), 3, "cell + its Y2 layer");
    assert_eq!((after.panels[1].row, after.panels[1].column), (1, 0));
    assert_eq!(after.panels[1].series.len(), 1);
    assert_eq!(after.panels[1].series[0].y, c[1]);
    assert_eq!((after.panels[2].row, after.panels[2].column), (1, 0));
    assert!(after.panels[2].overlay.is_some());
    assert_eq!(after.panels[2].series[0].y, c[3]);
    assert_eq!(
        after.panels[0].series.len(),
        1,
        "the first cell is untouched"
    );

    state.dispatch(Action::Undo);
    assert_eq!(graph_spec(&state, graph), before, "a single step");
}

/// #357: deleting a layer node removes the cell with its overlays in one
/// step; the last cell is refused with a message and no history entry.
#[test]
fn removing_a_cell_is_one_step_and_the_last_cell_is_refused() {
    use stokesforge_app_state::object_manager::VisibilityTarget as T;
    use stokesforge_core::compose::CellSide;
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state, graph, c) = cell_bench(dir.path());

    let revision = state.snapshot().revision;
    state.dispatch(Action::RemovePlotObject {
        graph,
        target: T::Panel(0),
    });
    assert_eq!(graph_spec(&state, graph).panels.len(), 1);
    assert_eq!(state.snapshot().revision, revision, "no command sent");
    assert_eq!(
        state.status().expect("a message").text,
        stokesforge_app_state::tr!("error-last-layer")
    );

    state.dispatch(Action::AddCell {
        graph,
        panel: 0,
        side: CellSide::Right,
        columns: c.clone(),
    });
    let two_cells = graph_spec(&state, graph);
    assert_eq!(two_cells.panels.len(), 3);
    state.dispatch(Action::RemovePlotObject {
        graph,
        target: T::Panel(1),
    });
    let one = graph_spec(&state, graph);
    assert_eq!(one.panels.len(), 1, "cell and its role layer gone");
    assert_eq!((one.panels[0].row, one.panels[0].column), (0, 0));
    assert_eq!(
        state.snapshot().history.undo_label,
        Some(HistoryLabel::EditPlotSpec(
            stokesforge_commands::PlotEditKind::RemoveCell
        ))
    );
    state.dispatch(Action::Undo);
    assert_eq!(
        graph_spec(&state, graph),
        two_cells,
        "one Undo restores both"
    );
}

/// The layout of a spec as the exports measure it.
fn layout_of(spec: &stokesforge_plot::PlotSpec) -> stokesforge_render::pipeline::FigureLayout {
    stokesforge_render_svg::figure_layout(
        spec,
        &stokesforge_render_svg::SvgTextMeasurer::default(),
        &stokesforge_render_svg::FontChoice::default(),
    )
    .expect("layout")
}

/// Puts a spec into the model as an old file would have it, **without** the
/// application's door (which fixes the frames of a legacy spec): the session
/// handle executes the command as it stands.
fn put_legacy_spec(
    session: &Session,
    state: &mut AppState,
    graph: NodeId,
    spec: stokesforge_plot::PlotSpec,
) {
    assert!(spec.frame_widths.is_empty() && spec.frame_heights.is_empty());
    session
        .handle()
        .execute(stokesforge_commands::Command::SetPlotSpec { graph, spec })
        .expect("legacy spec set");
    state.refresh();
}

/// #394: a new graph is created with explicit frames, chosen so that its
/// figure is exactly 480 x 360 pt, and a cell added beside it has the same
/// frame: the figure grows by one cell, 4/3 each.
#[test]
fn a_new_graph_is_480_by_360_and_a_cell_beside_it_has_the_same_frame() {
    use stokesforge_core::compose::CellSide;
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state, graph, _c) = cell_bench(dir.path());
    let spec = graph_spec(&state, graph);
    assert_eq!(spec.frame_widths.len(), 1, "explicit from the start");
    assert_eq!(spec.frame_heights.len(), 1);
    assert_eq!(layout_of(&spec).size, (480, 360));
    let frame = layout_of(&spec).frame_rect(0, 0).expect("frame");

    state.dispatch(Action::AddCell {
        graph,
        panel: 0,
        side: CellSide::Right,
        columns: Vec::new(),
    });
    let wider = graph_spec(&state, graph);
    assert_eq!(wider.frame_widths, [spec.frame_widths[0]; 2]);
    let layout = layout_of(&wider);
    assert_eq!(layout.size, (960, 360));
    assert_eq!(
        layout.frame_rect(0, 1).map(|r| (r.2, r.3)),
        Some((frame.2, frame.3)),
        "the new frame is the reference's"
    );

    state.dispatch(Action::AddCell {
        graph,
        panel: 0,
        side: CellSide::Below,
        columns: Vec::new(),
    });
    assert_eq!(layout_of(&graph_spec(&state, graph)).size, (960, 720));
}

/// #394 (G7): a graph from an old file (nothing stored, the bands and the title
/// inside the 480 x 360) is fixed by its first edit, whatever the edit: removing
/// the title keeps the plot area exactly where and as large as it was, and the
/// figure shrinks by the band. Opening it wrote nothing.
#[test]
fn editing_a_legacy_graph_fixes_its_frames_and_removing_the_title_shrinks_the_image() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (session, mut state, graph, _c) = cell_bench(dir.path());
    let mut legacy = graph_spec(&state, graph);
    legacy.title = "Legacy".to_owned();
    legacy.frame_widths.clear();
    legacy.frame_heights.clear();
    put_legacy_spec(&session, &mut state, graph, legacy.clone());
    let revision = state.snapshot().revision;
    let drawn = layout_of(&legacy);
    assert_eq!(drawn.size, (480, 360));
    assert!(drawn.title_band > 0 && !drawn.is_explicit());
    assert_eq!(
        graph_spec(&state, graph).frame_widths,
        Vec::<f64>::new(),
        "reading it fixes nothing"
    );
    assert_eq!(state.snapshot().revision, revision);

    let mut untitled = legacy.clone();
    untitled.title.clear();
    state.dispatch(Action::SetPlotSpec {
        graph,
        spec: untitled,
    });
    let after_spec = graph_spec(&state, graph);
    assert_eq!(after_spec.frame_widths, drawn.frame_widths, "frame fixed");
    assert_eq!(after_spec.frame_heights, drawn.frame_heights);
    let after = layout_of(&after_spec);
    assert_eq!(after.size, (480, 360 - drawn.title_band));
    assert_eq!(
        after.frame_rect(0, 0).map(|r| (r.2, r.3)),
        Some((drawn.frame_widths[0] as u32, drawn.frame_heights[0] as u32)),
        "the plot area is the legacy one"
    );
    // One edit, one undo step: the graph is legacy again.
    state.dispatch(Action::Undo);
    assert_eq!(graph_spec(&state, graph), legacy);
}

/// #394: adding a cell to a legacy graph keeps the exact plot area of the
/// existing cell and gives the new one the same.
#[test]
fn adding_a_cell_to_a_legacy_graph_keeps_the_frame_of_the_existing_cell() {
    use stokesforge_core::compose::CellSide;
    let dir = tempfile::tempdir().expect("temporary directory");
    let (session, mut state, graph, _c) = cell_bench(dir.path());
    let mut legacy = graph_spec(&state, graph);
    legacy.title = "Legacy".to_owned();
    legacy.frame_widths.clear();
    legacy.frame_heights.clear();
    put_legacy_spec(&session, &mut state, graph, legacy.clone());
    let before = layout_of(&legacy);

    state.dispatch(Action::AddCell {
        graph,
        panel: 0,
        side: CellSide::Right,
        columns: Vec::new(),
    });
    let spec = graph_spec(&state, graph);
    assert_eq!(spec.frame_widths[0], before.frame_widths[0]);
    assert_eq!(spec.frame_widths[1], before.frame_widths[0], "same frame");
    assert_eq!(spec.frame_heights, before.frame_heights);
    let after = layout_of(&spec);
    assert_eq!(after.size.1, 360, "same height, the title band is inside");
    assert_eq!(after.size.0, 480 * 2, "one more cell wide");
}

/// #394: a frame size set through `Action::SetFrameSize` is one undo step,
/// redo restores it, the other frames keep their size.
#[test]
fn setting_a_frame_size_is_one_undo_step_and_the_other_frames_keep_theirs() {
    use stokesforge_core::compose::{CellSide, FrameAxis};
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state, graph, _c) = cell_bench(dir.path());
    state.dispatch(Action::AddCell {
        graph,
        panel: 0,
        side: CellSide::Right,
        columns: Vec::new(),
    });
    let before = graph_spec(&state, graph);
    let revision = state.snapshot().revision;

    state.dispatch(Action::SetFrameSize {
        graph,
        axis: FrameAxis::Column,
        index: 1,
        size: 300.0,
    });
    let after = graph_spec(&state, graph);
    assert_eq!(after.frame_widths, [before.frame_widths[0], 300.0]);
    assert_eq!(after.frame_heights, before.frame_heights);
    assert_eq!(
        state.snapshot().history.undo_label,
        Some(HistoryLabel::EditPlotSpec(
            stokesforge_commands::PlotEditKind::ResizeCell
        ))
    );
    state.dispatch(Action::Undo);
    assert_eq!(graph_spec(&state, graph), before, "one Undo is enough");
    state.dispatch(Action::Redo);
    assert_eq!(graph_spec(&state, graph), after);

    // Same size, or outside the grid: nothing is sent.
    let revision_now = state.snapshot().revision;
    state.dispatch(Action::SetFrameSize {
        graph,
        axis: FrameAxis::Column,
        index: 1,
        size: 300.0,
    });
    state.dispatch(Action::SetFrameSize {
        graph,
        axis: FrameAxis::Row,
        index: 7,
        size: 100.0,
    });
    assert_eq!(state.snapshot().revision, revision_now);
    assert_ne!(revision, revision_now);
}

/// #394: pulling a corner handle commits both sizes as ONE undo step; a legacy
/// graph is given the frames it was drawn with, only the pulled ones move; a
/// refused size (outside the grid) commits nothing, not even the other one.
#[test]
fn pulling_a_corner_is_one_undo_step_and_fixes_a_legacy_graph() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let (session, mut state, graph, _c) = cell_bench(dir.path());
    let mut legacy = graph_spec(&state, graph);
    legacy.title = "Legacy".to_owned();
    legacy.frame_widths.clear();
    legacy.frame_heights.clear();
    put_legacy_spec(&session, &mut state, graph, legacy.clone());
    let drawn = layout_of(&legacy);

    state.dispatch(Action::SetFrameSizes {
        graph,
        column: Some((0, 300.0)),
        row: Some((0, 200.0)),
    });
    let after = graph_spec(&state, graph);
    assert_eq!(
        (after.frame_widths.clone(), after.frame_heights.clone()),
        (vec![300.0], vec![200.0])
    );
    assert_ne!(drawn.frame_widths, after.frame_widths);
    assert_eq!(
        state.snapshot().history.undo_label,
        Some(HistoryLabel::EditPlotSpec(
            stokesforge_commands::PlotEditKind::ResizeCell
        ))
    );
    state.dispatch(Action::Undo);
    assert_eq!(graph_spec(&state, graph), legacy, "one Undo is enough");
    state.dispatch(Action::Redo);
    assert_eq!(graph_spec(&state, graph), after);

    let revision = state.snapshot().revision;
    state.dispatch(Action::SetFrameSizes {
        graph,
        column: Some((0, 250.0)),
        row: Some((9, 200.0)),
    });
    assert_eq!(state.snapshot().revision, revision, "all or nothing");
}

/// #394: deleting a cell that empties its column removes that column; the
/// figure shrinks by the column's outer width, and undo brings both back in
/// one step.
#[test]
fn deleting_the_last_cell_of_a_column_shrinks_the_figure_by_its_outer_width() {
    use stokesforge_app_state::object_manager::VisibilityTarget as T;
    use stokesforge_core::compose::{CellSide, FrameAxis};
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state, graph, _c) = cell_bench(dir.path());
    let one = graph_spec(&state, graph);
    state.dispatch(Action::AddCell {
        graph,
        panel: 0,
        side: CellSide::Right,
        columns: Vec::new(),
    });
    state.dispatch(Action::SetFrameSize {
        graph,
        axis: FrameAxis::Column,
        index: 1,
        size: 300.0,
    });
    let two = graph_spec(&state, graph);
    // The new column: its frame (300) and the bands of the reference.
    assert_eq!(layout_of(&two).size, (480 + 300 + 24 + 64, 360));
    state.dispatch(Action::RemovePlotObject {
        graph,
        target: T::Panel(1),
    });
    assert_eq!(graph_spec(&state, graph).frame_widths, one.frame_widths);
    assert_eq!(layout_of(&graph_spec(&state, graph)).size, (480, 360));
    state.dispatch(Action::Undo);
    assert_eq!(graph_spec(&state, graph), two);
}

/// #357: deleting an overlay layer takes that layer only.
#[test]
fn removing_an_overlay_layer_leaves_its_cell() {
    use stokesforge_app_state::object_manager::VisibilityTarget as T;
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_session, mut state, graph, _c) = cell_bench(dir.path());
    state.dispatch(Action::AddLayer {
        graph,
        panel: 0,
        x_top: true,
        y_right: false,
    });
    assert_eq!(graph_spec(&state, graph).panels.len(), 2);
    state.dispatch(Action::RemovePlotObject {
        graph,
        target: T::Panel(1),
    });
    assert_eq!(graph_spec(&state, graph).panels.len(), 1);
}

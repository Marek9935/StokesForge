//! Ctrl+Z, Ctrl+Y and Ctrl+S in the detached windows (#459, work package #470).
//!
//! The detached graph, workbook and the three panels read these three keys
//! before they paint, and queue the actions after their own; Ctrl+Q quits
//! nothing there. The tests drive a root and the five windows the way eframe
//! does (`test_support::bench::Bench::with_every_window`).
use std::time::Instant;

use eframe::egui;
use stokesforge_app_state::state::{Action, AppState};
use stokesforge_app_state::tr;

use super::project_manager;
use super::tab::Tab;
use super::test_support::bench::{Bench, Pass, Window};
use super::test_support::{key, text_exact, texts};
use super::window_band;

fn chord(key: egui::Key, modifiers: egui::Modifiers) -> egui::Event {
    egui::Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers,
    }
}

fn ctrl(key: egui::Key) -> egui::Event {
    chord(key, egui::Modifiers::COMMAND)
}

fn ctrl_shift(key: egui::Key) -> egui::Event {
    chord(key, egui::Modifiers::COMMAND.plus(egui::Modifiers::SHIFT))
}

fn bench() -> Bench {
    let mut bench = Bench::with_every_window();
    bench.start_every_window();
    bench
}

/// `event` pressed in `window` for one frame, then the root drains it.
fn press(bench: &mut Bench, window: Window, event: egui::Event) {
    bench.child_in(bench.id_of(window), Pass::child().events(vec![event]));
    settle(bench);
}

/// The root drains what the windows sent up, and the windows see the result.
fn settle(bench: &mut Bench) {
    for _ in 0..2 {
        bench.root(Pass::root());
    }
    for window in Window::ALL {
        bench.child_in(bench.id_of(window), Pass::child().focused(false));
    }
}

fn books(bench: &Bench) -> usize {
    bench.app.state.books().len()
}

/// A model step to have something to undo: one more workbook.
fn step(bench: &mut Bench) {
    bench.app.state.dispatch(Action::AddWorkbook);
}

fn band_in(bench: &Bench, window: Window) -> Option<String> {
    window_band::text_at(&bench.ctx, bench.id_of(window), Instant::now()).map(|(text, _)| text)
}

fn with_view<R>(bench: &Bench, f: impl FnOnce(&mut AppState) -> R) -> R {
    let detached = &bench.app.detached_tabs[&Tab::Book(bench.book())];
    let mut model = detached.view_model.lock().unwrap();
    f(model.view_state.as_mut().expect("the local view"))
}

/// The cell A1 of the detached workbook is the selected one.
fn select_first_cell(bench: &Bench) {
    let book = bench.book();
    with_view(bench, |view| {
        let sheet = view.selected_sheet_in(book).unwrap();
        let column = view.tree().children(sheet).unwrap()[0];
        view.select_cell(sheet, column, 0);
    });
}

fn typed_cell(bench: &Bench) -> Option<String> {
    with_view(bench, |view| view.editing().map(|edit| edit.buffer.clone()))
}

fn closes(output: &egui::FullOutput, id: egui::ViewportId) -> bool {
    output.viewport_output[&id]
        .commands
        .iter()
        .any(|command| matches!(command, egui::ViewportCommand::Close))
}

fn click(bench: &mut Bench, window: Window, pos: egui::Pos2) {
    let id = bench.id_of(window);
    for events in [
        vec![egui::Event::PointerMoved(pos)],
        vec![egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        }],
        vec![egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        }],
    ] {
        bench.child_in(id, Pass::child().events(events));
    }
}

/// Ctrl+Z, Ctrl+Y and Ctrl+Shift+Z pressed in each window move the history by
/// exactly one step: one press is one action on one stack, nothing is undone
/// twice. Witness: stop reading the keys in one closure; or read them a second
/// time in one of them (two steps move at once).
#[test]
fn ctrl_z_and_ctrl_y_in_each_detached_window_move_the_history_by_one_step() {
    let mut bench = bench();
    for window in Window::ALL {
        let base = books(&bench);
        step(&mut bench);
        step(&mut bench);
        press(&mut bench, window, ctrl(egui::Key::Z));
        assert_eq!(books(&bench), base + 1, "{window:?}: Ctrl+Z, one step");
        press(&mut bench, window, ctrl(egui::Key::Y));
        assert_eq!(books(&bench), base + 2, "{window:?}: Ctrl+Y, one step");
        press(&mut bench, window, ctrl(egui::Key::Z));
        press(&mut bench, window, ctrl_shift(egui::Key::Z));
        assert_eq!(books(&bench), base + 2, "{window:?}: Ctrl+Shift+Z redoes");
        for _ in 0..2 {
            bench.app.state.dispatch(Action::Undo);
        }
        assert_eq!(books(&bench), base);
    }
    bench.finish();
}

/// Ctrl+S in each window sends one Save and the file is written. The project
/// has a file here: no picker opens. Witness: drop Save from the reader.
#[test]
fn ctrl_s_in_each_detached_window_saves_the_project() {
    let mut bench = bench();
    for window in Window::ALL {
        let _ = std::fs::remove_file(bench.path());
        bench.child_in(
            bench.id_of(window),
            Pass::child().events(vec![ctrl(egui::Key::S)]),
        );
        assert_eq!(
            bench.queued_by(window),
            vec![Action::Save],
            "{window:?}: one Save is queued"
        );
        settle(&mut bench);
        assert!(bench.path().exists(), "{window:?}: the file was written");
        assert_eq!(band_in(&bench, window), None, "{window:?}: no band");
    }
    bench.finish();
}

/// Ctrl+Shift+S in a detached window does nothing: it is consumed and dropped,
/// so Save As never saves silently. Witness: remove the discard in
/// `read_detached_history_and_save` (Ctrl+S then matches it).
#[test]
fn ctrl_shift_s_in_a_detached_window_saves_nothing() {
    let mut bench = bench();
    for window in Window::ALL {
        bench.child_in(
            bench.id_of(window),
            Pass::child().events(vec![ctrl_shift(egui::Key::S)]),
        );
        assert!(
            bench.queued_by(window).is_empty(),
            "{window:?}: nothing queued"
        );
        settle(&mut bench);
        assert!(!bench.path().exists(), "{window:?}: nothing saved");
        assert!(!bench.app.file_picker.is_open(), "{window:?}: no picker");
    }
    bench.finish();
}

/// With no file yet, Ctrl+S queues the Save and writes nothing: the root then
/// asks where (`dispatch_actions`; never a real picker in a test, so the root
/// does not run here). Witness: drop Save from the reader.
#[test]
fn ctrl_s_with_no_file_yet_queues_the_save_and_writes_nothing() {
    let mut bench = Bench::new();
    bench.start();
    bench.child(Pass::child().events(vec![ctrl(egui::Key::S)]));
    let queued = bench.app.detached_tabs[&bench.tab]
        .requests
        .lock()
        .unwrap()
        .actions
        .clone();
    assert_eq!(queued, vec![Action::Save]);
    assert!(bench.app.state.path().is_empty());
    bench.finish();
}

/// A value typed in a detached cell stays when Ctrl+Z is pressed: the command
/// is undone, the buffer is left as it is (one press, one action). Witness:
/// read the keys after the contents are painted in `show_locked` (the cell
/// takes Ctrl+Z first and undoes its text).
#[test]
fn ctrl_z_with_a_value_typed_in_a_detached_cell_undoes_the_command_not_the_text() {
    let mut bench = bench();
    step(&mut bench);
    let base = books(&bench);
    select_first_cell(&bench);
    let book = bench.id_of(Window::Book);
    bench.child_in(
        book,
        Pass::child().events(vec![egui::Event::Text("7".into())]),
    );
    for _ in 0..2 {
        bench.child_in(book, Pass::child());
    }
    bench.child_in(
        book,
        Pass::child().events(vec![egui::Event::Text("7".into())]),
    );
    bench.child_in(book, Pass::child());
    let typed = typed_cell(&bench).expect("the cell is being edited");
    assert!(typed.ends_with("77"), "typed: {typed:?}");
    press(&mut bench, Window::Book, ctrl(egui::Key::Z));
    assert_eq!(books(&bench), base - 1, "the command was undone");
    assert_eq!(typed_cell(&bench), Some(typed), "the typed text is left");
    bench.finish();
}

/// The console: Ctrl+Z with the cursor in the script undoes the command and
/// leaves the script as typed (card S1). Witness: read after painting. Only
/// with the `python` feature: without it the script field is disabled and
/// cannot take the cursor.
#[cfg(feature = "python")]
#[test]
fn ctrl_z_with_the_cursor_in_the_detached_console_undoes_the_command_not_the_text() {
    let mut bench = bench();
    step(&mut bench);
    let base = books(&bench);
    let console = bench.id_of(Window::Console);
    // Click into the script, then type.
    for y in [60.0, 90.0, 120.0, 150.0] {
        click(&mut bench, Window::Console, egui::pos2(300.0, y));
        if bench.ctx.memory(|memory| memory.focused()).is_some() {
            break;
        }
    }
    assert!(
        bench.ctx.memory(|memory| memory.focused()).is_some(),
        "the cursor is in a field"
    );
    bench.child_in(
        console,
        Pass::child().events(vec![egui::Event::Text("abc".into())]),
    );
    bench.child_in(console, Pass::child());
    settle(&mut bench);
    bench.child_in(console, Pass::child());
    let script = bench.app.state.script_source().to_string();
    assert!(script.contains("abc"), "typed: {script:?}");
    press(&mut bench, Window::Console, ctrl(egui::Key::Z));
    assert_eq!(books(&bench), base - 1, "the command was undone");
    assert_eq!(
        bench.app.state.script_source(),
        script,
        "the script is left"
    );
    bench.finish();
}

/// The name field of the detached Project Manager: same rule. Witness: read
/// after painting.
#[test]
fn ctrl_z_with_a_name_typed_in_the_detached_project_manager_undoes_the_command_not_the_text() {
    let mut bench = bench();
    let root = bench.app.state.tree().root();
    bench.app.state.dispatch(Action::AddFolder { parent: root });
    let folder = *bench
        .app
        .state
        .tree()
        .node(root)
        .unwrap()
        .children()
        .last()
        .unwrap();
    step(&mut bench);
    let base = books(&bench);
    let pm = bench.id_of(Window::ProjectManager);
    project_manager::begin_rename_of_new_folder(&bench.ctx, pm, folder, root);
    settle(&mut bench);
    bench.child_in(
        pm,
        Pass::child().events(vec![egui::Event::Text("abc".into())]),
    );
    bench.child_in(pm, Pass::child());
    let typed = project_manager::rename_text_in(&bench.ctx, pm).expect("a name is typed");
    assert!(typed.contains("abc"), "typed: {typed:?}");
    press(&mut bench, Window::ProjectManager, ctrl(egui::Key::Z));
    assert_eq!(books(&bench), base - 1, "the command was undone");
    assert_eq!(project_manager::renaming_in(&bench.ctx, pm), vec![folder]);
    assert_eq!(
        project_manager::rename_text_in(&bench.ctx, pm),
        Some(typed),
        "the typed name is left"
    );
    bench.finish();
}

/// Add Folder in the detached Project Manager, then Ctrl+Z: the folder is gone
/// and so is its name field (the arrows work again). Witness: remove the
/// pruning of `memory.rename` in `project_manager_contents_from_tree`.
#[test]
fn undoing_add_folder_in_the_detached_project_manager_closes_its_name_field() {
    let mut bench = bench();
    let root = bench.app.state.tree().root();
    let pm = bench.id_of(Window::ProjectManager);
    bench
        .app
        .pm_requests
        .lock()
        .unwrap()
        .actions
        .push(Action::AddFolder { parent: root });
    settle(&mut bench);
    for _ in 0..3 {
        bench.child_in(pm, Pass::child());
    }
    assert_eq!(
        project_manager::renaming_in(&bench.ctx, pm).len(),
        1,
        "the name of the new folder is being edited"
    );
    press(&mut bench, Window::ProjectManager, ctrl(egui::Key::Z));
    bench.child_in(pm, Pass::child());
    assert!(
        project_manager::renaming_in(&bench.ctx, pm).is_empty(),
        "the field closed with its folder"
    );
    bench.finish();
}

/// With a dialog open in the main window, Ctrl+Z, Ctrl+Y and Ctrl+S with the
/// cursor in a detached field change nothing: not the model, not the file, not
/// the field. The orange band says why. Witness: let the key through (drop the
/// consumption), or stop consuming it (the field undoes its text).
#[test]
fn a_dialog_in_the_main_window_refuses_the_three_keys_even_with_the_cursor_in_a_field() {
    let mut bench = bench();
    step(&mut bench);
    select_first_cell(&bench);
    let book = bench.id_of(Window::Book);
    bench.child_in(
        book,
        Pass::child().events(vec![egui::Event::Text("7".into())]),
    );
    for _ in 0..2 {
        bench.child_in(book, Pass::child());
    }
    bench.child_in(
        book,
        Pass::child().events(vec![egui::Event::Text("7".into())]),
    );
    bench.child_in(book, Pass::child());
    let typed = typed_cell(&bench).expect("the cursor is in a cell");
    bench.app.state.dispatch(Action::OpenAbout);
    bench.root(Pass::root());
    let (base, revision) = (books(&bench), bench.app.state.revision());
    for (event, what) in [
        (ctrl(egui::Key::Z), "Ctrl+Z"),
        (ctrl(egui::Key::Y), "Ctrl+Y"),
        (ctrl(egui::Key::S), "Ctrl+S"),
    ] {
        window_band::clear(&bench.ctx, book);
        press(&mut bench, Window::Book, event);
        assert_eq!(books(&bench), base, "{what}: model unchanged");
        assert_eq!(bench.app.state.revision(), revision, "{what}: revision");
        assert!(!bench.path().exists(), "{what}: nothing saved");
        assert_eq!(typed_cell(&bench), Some(typed.clone()), "{what}: the field");
        let band = band_in(&bench, Window::Book).unwrap_or_default();
        assert!(band.contains("main window"), "{what}: band {band:?}");
    }
    bench.finish();
}

/// Ctrl+Z with nothing to undo shows the orange band in that window only.
/// Witness: lose the origin of the gesture.
#[test]
fn ctrl_z_with_an_empty_history_says_so_in_that_window_only() {
    let mut bench = bench();
    assert!(bench.app.state.undo_label().is_none());
    press(&mut bench, Window::Book, ctrl(egui::Key::Z));
    let band = band_in(&bench, Window::Book).expect("the band of the workbook window");
    assert!(!band.is_empty());
    for other in [Window::Graph, Window::ProjectManager, Window::Console] {
        assert_eq!(band_in(&bench, other), None, "{other:?}: no band");
    }
    bench.finish();
}

/// In the Page Layout Mode of a detached graph, Ctrl+S saves, the mode stays,
/// Esc still leaves it. Witness: let the mode consume Ctrl+S, or leave the mode
/// when it saves.
#[test]
fn ctrl_s_in_the_page_layout_mode_saves_and_keeps_the_mode() {
    let mut bench = bench();
    let graph = bench.graph;
    bench.app.toggle_layout_mode(graph, Some(true));
    for _ in 0..4 {
        bench.root(Pass::root());
        bench.child_in(bench.id_of(Window::Graph), Pass::child());
    }
    let in_mode = |bench: &Bench| {
        bench.app.detached_tabs[&bench.tab]
            .view_model
            .lock()
            .unwrap()
            .graph_view
            .as_ref()
            .is_some_and(|view| view.layout_mode())
    };
    assert!(in_mode(&bench));
    press(&mut bench, Window::Graph, ctrl(egui::Key::S));
    assert!(bench.path().exists(), "saved");
    assert!(in_mode(&bench), "the mode stays");
    let graph_window = bench.id_of(Window::Graph);
    // The window has the keyboard back and the root has seen it.
    bench.child_in(graph_window, Pass::child());
    bench.root(Pass::root());
    bench.child_in(graph_window, Pass::child());
    bench.child_in(
        graph_window,
        Pass::child().events(vec![key(egui::Key::Escape)]),
    );
    bench.root(Pass::root());
    assert!(!in_mode(&bench), "Esc still leaves the mode");
    bench.finish();
}

/// A gesture of the strip and Ctrl+Z in the same frame, in the Page Layout
/// Mode: the key undoes that very gesture. Witness: queue the key before the
/// gesture's actions.
#[test]
fn ctrl_z_pressed_with_a_strip_click_in_the_same_frame_undoes_the_click() {
    let mut bench = bench();
    let graph = bench.graph;
    let id = bench.id_of(Window::Graph);
    bench.app.toggle_layout_mode(graph, Some(true));
    for _ in 0..4 {
        bench.root(Pass::root());
        bench.child_in(id, Pass::child());
    }
    let fixed = |bench: &Bench| bench.app.state.plot_spec(graph).unwrap().fixed_figure;
    let before = fixed(&bench);
    let last = bench.child_in(id, Pass::child());
    let pos = text_exact(&texts(&last.shapes), &tr!("plot-figure-size-fixed")).center;
    bench.child_in(
        id,
        Pass::child().events(vec![egui::Event::PointerMoved(pos)]),
    );
    let both = vec![
        egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        },
        egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        },
        ctrl(egui::Key::Z),
    ];
    bench.child_in(id, Pass::child().events(both));
    let queued = bench.queued_by(Window::Graph);
    assert_eq!(queued.len(), 2, "the click, then the key: {queued:?}");
    assert!(matches!(queued[0], Action::SetFigureFixed { .. }));
    assert_eq!(queued[1], Action::Undo);
    settle(&mut bench);
    assert_eq!(fixed(&bench), before, "the click was undone");
    bench.finish();
}

/// Ctrl+Q in a detached window closes nothing and says where Quit is; a value
/// typed in a cell stays. Witness: remove `refuse_quit_in_detached` from a
/// closure (egui closes the window at the end of the pass).
#[test]
fn ctrl_q_in_a_detached_window_closes_nothing_and_says_where_quit_is() {
    let mut bench = bench();
    for window in Window::ALL {
        let id = bench.id_of(window);
        window_band::clear(&bench.ctx, id);
        let output = bench.child_in(id, Pass::child().events(vec![ctrl(egui::Key::Q)]));
        assert!(!closes(&output, id), "{window:?}: nothing closes");
        assert_eq!(
            band_in(&bench, window),
            Some(tr!("refused-quit-in-main")),
            "{window:?}: the band"
        );
        assert!(bench.queued_by(window).is_empty(), "{window:?}: no action");
    }
    // A value typed in a cell stays.
    select_first_cell(&bench);
    let book = bench.id_of(Window::Book);
    bench.child_in(
        book,
        Pass::child().events(vec![egui::Event::Text("7".into())]),
    );
    bench.child_in(book, Pass::child());
    let typed = typed_cell(&bench).expect("being edited");
    let output = bench.child_in(book, Pass::child().events(vec![ctrl(egui::Key::Q)]));
    assert!(!closes(&output, book));
    assert_eq!(typed_cell(&bench), Some(typed), "the typed value stays");
    bench.finish();
}

/// Ctrl+Q in the main window still asks to close it (the Quit question).
/// Witness: consume the quit shortcut in the root too.
#[test]
fn ctrl_q_in_the_main_window_still_quits() {
    let mut bench = bench();
    let output = bench.root(Pass::root().focused(true).events(vec![ctrl(egui::Key::Q)]));
    assert!(closes(&output, egui::ViewportId::ROOT));
    bench.finish();
}

fn keys_of_a_surface() -> Vec<egui::Event> {
    vec![
        key(egui::Key::Escape),
        key(egui::Key::Delete),
        key(egui::Key::ArrowDown),
        key(egui::Key::F2),
        egui::Event::Text("7".into()),
    ]
}

/// #460, detached workbook. With a dialog open in the main window, Esc, Del,
/// the arrows, F2 and typing do nothing in the grid of a detached workbook:
/// no edit opens, the selection stays, the model is untouched and the window's
/// band says why. Without the dialog the same keys do act (the witness that the
/// test is not vacuous). Witness: remove `stop_keys` from
/// `DetachedTabViewport::show`.
#[test]
fn a_dialog_in_the_main_window_stops_the_keys_of_a_detached_workbook_grid() {
    let mut bench = bench();
    select_first_cell(&bench);
    let book = bench.id_of(Window::Book);
    let cell = |bench: &Bench| {
        let book_id = bench.book();
        with_view(bench, |view| {
            let sheet = view.selected_sheet_in(book_id).unwrap();
            view.selection(sheet).cloned()
        })
    };
    let selection = cell(&bench);
    // No dialog: typing does open an edit.
    bench.child_in(book, Pass::child());
    bench.child_in(
        book,
        Pass::child().events(vec![egui::Event::Text("7".into())]),
    );
    assert!(
        typed_cell(&bench).is_some(),
        "without a dialog, typing edits"
    );
    bench.child_in(book, Pass::child().events(vec![key(egui::Key::Escape)]));
    bench.child_in(book, Pass::child());
    assert!(typed_cell(&bench).is_none(), "Esc cancels it");
    // A dialog in the main window.
    bench.app.state.dispatch(Action::OpenAbout);
    bench.root(Pass::root());
    let (revision, history) = (
        bench.app.state.revision(),
        bench.app.state.status_history().count(),
    );
    bench.child_in(book, Pass::child().events(keys_of_a_surface()));
    bench.child_in(book, Pass::child());
    assert!(typed_cell(&bench).is_none(), "no edit opened");
    assert_eq!(cell(&bench), selection, "the selection stayed");
    assert!(bench.queued_by(Window::Book).is_empty(), "nothing queued");
    bench.root(Pass::root());
    assert_eq!(
        bench.app.state.revision(),
        revision,
        "the model is untouched"
    );
    bench.child_in(book, Pass::child());
    let band = band_in(&bench, Window::Book).expect("the band of the window");
    assert!(band.contains("main window"), "{band}");
    assert_eq!(bench.app.state.status_history().count(), history + 1);
    bench.finish();
}

/// #460, detached Project Manager. With a dialog open in the main window, Esc,
/// Del, the arrows, F2 and typing do nothing in the detached Project Manager:
/// no name field opens, the folder stays, the selection stays, the band shows.
/// Without the dialog F2 does open the field. Witness: remove `stop_keys` from
/// the Project Manager closure.
#[test]
fn a_dialog_in_the_main_window_stops_the_keys_of_the_detached_project_manager() {
    let mut bench = bench();
    let root = bench.app.state.tree().root();
    bench.app.state.dispatch(Action::AddFolder { parent: root });
    let folder = *bench
        .app
        .state
        .tree()
        .node(root)
        .unwrap()
        .children()
        .last()
        .unwrap();
    settle(&mut bench);
    let pm = bench.id_of(Window::ProjectManager);
    let name = bench
        .app
        .state
        .tree()
        .node(folder)
        .unwrap()
        .name()
        .to_string();
    // Select the folder with a click on its row.
    let last = bench.child_in(pm, Pass::child());
    let row = text_exact(&texts(&last.shapes), &name).center;
    click(&mut bench, Window::ProjectManager, row);
    // The panel keeps the keyboard only while its window is the focused one.
    for _ in 0..2 {
        bench.root(Pass::root());
        bench.child_in(pm, Pass::child());
    }
    // No dialog: F2 opens the name field.
    bench.child_in(pm, Pass::child().events(vec![key(egui::Key::F2)]));
    bench.child_in(pm, Pass::child());
    assert_eq!(
        project_manager::renaming_in(&bench.ctx, pm),
        vec![folder],
        "without a dialog, F2 renames"
    );
    bench.child_in(pm, Pass::child().events(vec![key(egui::Key::Escape)]));
    bench.child_in(pm, Pass::child());
    assert!(project_manager::renaming_in(&bench.ctx, pm).is_empty());
    // A dialog in the main window.
    bench.app.state.dispatch(Action::OpenAbout);
    bench.root(Pass::root());
    let (revision, selection) = (bench.app.state.revision(), bench.app.pm_selection.clone());
    bench.child_in(pm, Pass::child().events(keys_of_a_surface()));
    bench.child_in(pm, Pass::child());
    assert!(project_manager::renaming_in(&bench.ctx, pm).is_empty());
    assert!(bench.queued_by(Window::ProjectManager).is_empty());
    settle(&mut bench);
    assert_eq!(bench.app.state.revision(), revision, "the folder stays");
    assert_eq!(bench.app.pm_selection, selection, "the selection stays");
    let band = band_in(&bench, Window::ProjectManager).expect("the band of the window");
    assert!(band.contains("main window"), "{band}");
    bench.finish();
}

/// The cursor `(column index, row)` of the book's selected sheet in `state`.
type Cursor = Option<(usize, usize)>;

fn cursor_of(state: &mut AppState, book: stokesforge_core::NodeId) -> Option<(usize, usize)> {
    use stokesforge_app_state::selection::GridCursor;
    let sheet = state.selected_sheet_in(book)?;
    let tree = std::sync::Arc::clone(state.tree());
    let GridCursor::Data(column, row) = state.selection(sheet)?.anchor(&tree, sheet)? else {
        return None;
    };
    let index = tree
        .children(sheet)
        .ok()?
        .iter()
        .position(|c| *c == column)?;
    Some((index, row))
}

/// Puts the cursor on `(column index, row)` of the fork, types `7` and presses
/// `keys`; returns the cursor on the fork and on the root once settled.
fn type_and_press(
    bench: &mut Bench,
    from: (usize, usize),
    keys: Vec<egui::Event>,
) -> (Cursor, Cursor) {
    let book = bench.book();
    let id = bench.id_of(Window::Book);
    with_view(bench, |view| {
        let sheet = view.selected_sheet_in(book).unwrap();
        let column = view.tree().children(sheet).unwrap()[from.0];
        view.select_cell(sheet, column, from.1);
    });
    bench.child_in(id, Pass::child());
    bench.child_in(
        id,
        Pass::child().events(vec![
            egui::Event::ModifiersChanged(egui::Modifiers::NONE),
            egui::Event::Text("7".into()),
        ]),
    );
    bench.child_in(id, Pass::child());
    bench.child_in(id, Pass::child());
    assert!(typed_cell(bench).is_some(), "being edited");
    bench.child_in(id, Pass::child().events(keys));
    settle(bench);
    bench.child_in(id, Pass::child());
    (
        with_view(bench, |view| cursor_of(view, book)),
        cursor_of(&mut bench.app.state, book),
    )
}

/// Enter, Shift+Enter and Tab in a cell of a detached workbook, no dialog: the
/// cursor moves like in the main window, on the fork and on the root. Enter
/// used to move the root only, and the fork's stale selection overwrote it at
/// the next adoption. Witness: remove `return_selection_to_detached_books`
/// from the root pass (Enter cases fail; Tab and Shift+Enter keep passing).
#[test]
fn enter_tab_and_shift_enter_in_a_detached_cell_move_the_cursor_like_the_main_window() {
    let mut bench = bench();
    let book = bench.book();
    let (rows, entry) = {
        let state = &mut bench.app.state;
        let sheet = state.selected_sheet_in(book).unwrap();
        (state.entry_row(sheet), state.entry_row(sheet))
    };
    assert!(rows >= 2, "the sheet has data");
    let at = |what: &str, got: (Cursor, Cursor), want| {
        assert_eq!(got.0, Some(want), "{what}: the fork");
        assert_eq!(got.1, Some(want), "{what}: the root");
    };
    let got = type_and_press(&mut bench, (0, 0), vec![key(egui::Key::Enter)]);
    at("Enter", got, (0, 1));
    let got = type_and_press(
        &mut bench,
        (0, 1),
        vec![
            egui::Event::ModifiersChanged(egui::Modifiers::SHIFT),
            chord(egui::Key::Enter, egui::Modifiers::SHIFT),
        ],
    );
    at("Shift+Enter", got, (0, 0));
    let got = type_and_press(&mut bench, (0, 0), vec![key(egui::Key::Tab)]);
    at("Tab", got, (1, 0));
    // The last data row: Enter lands on the entry row; a value typed there adds
    // the row and the cursor lands on the new entry row.
    let got = type_and_press(&mut bench, (0, entry - 1), vec![key(egui::Key::Enter)]);
    at("Enter on the last row", got, (0, entry));
    let got = type_and_press(&mut bench, (0, entry), vec![key(egui::Key::Enter)]);
    at("Enter in the entry row", got, (0, entry + 1));
    let state = &mut bench.app.state;
    let sheet = state.selected_sheet_in(book).unwrap();
    assert_eq!(state.entry_row(sheet), entry + 1, "the row was added");
    bench.finish();
}

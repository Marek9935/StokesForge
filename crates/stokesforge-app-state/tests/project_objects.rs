//! #105: Project Manager decisions without a display server.

use stokesforge_app_state::state::{Action, AppState, StatusLevel};
use stokesforge_app_state::tr;
use stokesforge_commands::{HistoryLabel, MacroKind};
use stokesforge_core::{Column, NodeId, NodeKind, Project};
use stokesforge_plot::{PanelSpec, PlotSpec, SeriesSpec};
use stokesforge_session::Session;

fn fixture() -> (Session, AppState, NodeId, NodeId, NodeId) {
    let mut project = Project::new("Test").unwrap();
    let root = project.root();
    let book = project.add_workbook(root, Some("Book1")).unwrap();
    let sheet = project.tree().children(book).unwrap()[0];
    let x = project
        .add_column(sheet, None, Some("A"), Column::from_f64(&[1., 2.]))
        .unwrap();
    let y = project
        .add_column(sheet, None, Some("B"), Column::from_f64(&[3., 4.]))
        .unwrap();
    let mut panel = PanelSpec::default();
    panel.series.push(SeriesSpec::new_xy(x, y));
    let graph = project
        .add_graph(
            root,
            Some("Graph1"),
            PlotSpec::default().with_panels(stokesforge_plot::Panels::one(panel)),
        )
        .unwrap();
    let session = Session::spawn(project, None).unwrap();
    let state = AppState::new_with_preferences(
        session.handle(),
        String::new(),
        stokesforge_app_state::preferences::Preferences::load_from(None),
    )
    .unwrap();
    (session, state, book, graph, y)
}

#[test]
fn grouped_multiple_duplication_and_graphs_on_the_original_columns() {
    let (_session, mut state, book, graph, y) = fixture();
    let before = state.tree().clone();
    state.dispatch(Action::DuplicateObjects(vec![graph, book, book]));
    let created = state.take_copied_objects();
    assert_eq!(created.len(), 2);
    assert_eq!(
        state.snapshot().history.undo_label,
        Some(HistoryLabel::Macro(MacroKind::DuplicateObjects))
    );
    let book_copy = created
        .iter()
        .find(|(source, _, _)| *source == book)
        .unwrap()
        .1;
    let graph_copy = created
        .iter()
        .find(|(source, _, _)| *source == graph)
        .unwrap()
        .1;
    assert_eq!(state.tree().node(book_copy).unwrap().name(), "Book1 copy 1");
    assert!(state
        .tree()
        .walk(book_copy)
        .unwrap()
        .iter()
        .all(|id| !before.contains(*id)));
    let NodeKind::Graph { spec, .. } = state.tree().node(graph_copy).unwrap().kind() else {
        panic!("graph")
    };
    assert!(spec.references(y));
    state.dispatch(Action::Undo);
    assert_eq!(state.tree(), &before);
    state.dispatch(Action::Redo);
    assert!(state.tree().contains(book_copy));
    assert!(state.tree().contains(graph_copy));
}

#[test]
fn frozen_clipboard_and_paste_between_projects_with_destination_identifiers() {
    let (_source, mut source, book, _, y) = fixture();
    source.dispatch(Action::CopyObjects(vec![book]));
    let text = source.take_object_clipboard_text().unwrap();
    source.dispatch(Action::CommitCell {
        column: y,
        row: 0,
        text: "99".into(),
    });
    // The source IDs deliberately overlap with the recipient's.
    let (_destination, mut destination, _, _, _) = fixture();
    let parent = destination.tree().root();
    let before = destination.tree().clone();
    destination.dispatch(Action::PasteObjects {
        parent,
        text: text.clone(),
    });
    let copy = destination.take_copied_objects()[0].1;
    let sheet = destination.tree().children(copy).unwrap()[0];
    let columns = destination.tree().children(sheet).unwrap();
    assert_eq!(
        destination
            .tree()
            .node(columns[1])
            .unwrap()
            .as_column()
            .unwrap()
            .get(0),
        stokesforge_core::Cell::Float(3.)
    );
    assert!(destination
        .tree()
        .walk(copy)
        .unwrap()
        .iter()
        .all(|id| !before.contains(*id)));
    destination.dispatch(Action::PasteObjects { parent, text });
    let second = destination.take_copied_objects()[0].1;
    assert_eq!(
        destination.tree().node(second).unwrap().name(),
        "Book1 copy 2"
    );
    assert_ne!(copy, second);
    destination.dispatch(Action::Undo);
    destination.dispatch(Action::Undo);
    assert_eq!(destination.tree(), &before);
}

/// S-F3 (#229): an undoable deletion asks nothing; the message names the
/// **remaining** graphs that used its data, and a single Undo gives
/// everything back.
#[test]
fn deletion_without_confirmation_names_the_graphs_and_a_single_undo_restores_everything() {
    let (_session, mut state, book, graph, _) = fixture();
    let before = state.tree().clone();
    assert_eq!(state.object_users(&[book]), vec!["Graph1"]);
    assert!(
        state.object_users(&[book, graph]).is_empty(),
        "a graph deleted with its data is not named"
    );
    state.dispatch(Action::DeleteObjects(vec![book]));
    assert!(!state.tree().contains(book));
    assert!(state.unsaved_pending().is_none() && !state.blocking_dialog());
    let status = state.status().expect("message");
    assert!(!status.is_error());
    assert!(status.text.contains("Graph1"), "{}", status.text);
    state.dispatch(Action::Undo);
    assert_eq!(state.tree(), &before);
    state.dispatch(Action::DeleteObjects(vec![book, graph]));
    assert!(!state.tree().contains(book));
    assert!(!state.tree().contains(graph));
    state.dispatch(Action::Undo);
    assert_eq!(state.tree(), &before);
    state.dispatch(Action::DeleteObjects(vec![graph]));
    assert!(!state.tree().contains(graph));
}

#[test]
fn local_rename_and_unambiguous_paths_after_saving() {
    let (_session, mut state, book, graph, y) = fixture();
    let before = state.tree().clone();
    state.dispatch(Action::RenameObject {
        id: book,
        name: "Graph1".into(),
    });
    assert_eq!(state.tree(), &before);
    state.dispatch(Action::RenameObject {
        id: book,
        name: " ".into(),
    });
    assert_eq!(state.tree(), &before);
    state.dispatch(Action::RenameObject {
        id: book,
        name: "Measurements".into(),
    });
    assert!(state.plot_spec(graph).unwrap().references(y));
    state.dispatch(Action::Undo);
    assert_eq!(state.tree(), &before);
}

#[test]
fn pasteable_graph_in_its_project_and_embeds_its_data_in_another() {
    let (_session, mut state, _, graph, y) = fixture();
    state.dispatch(Action::CopyObjects(vec![graph]));
    let text = state.take_object_clipboard_text().unwrap();
    let parent = state.tree().root();
    state.dispatch(Action::PasteObjects {
        parent,
        text: text.clone(),
    });
    let copied_graph = state.take_copied_objects()[0].1;
    assert!(state.plot_spec(copied_graph).unwrap().references(y));
    let (_other, mut other, _, _, _) = fixture();
    let before = other.tree().clone();
    let parent = other.tree().root();
    other.dispatch(Action::PasteObjects { parent, text });
    let copied = other.take_copied_objects()[0].1;
    assert!(!other.plot_spec(copied).unwrap().references(y));
    let books = stokesforge_app_state::state::graph_data_books(other.tree(), copied);
    assert_eq!(books.len(), 1);
    let book = books[0];
    assert!(
        matches!(other.tree().node(book).unwrap().kind(), NodeKind::Workbook { meta, .. } if meta.hidden)
    );
    let entry = stokesforge_app_state::state::ProjectTreeNode::Window {
        id: book,
        name: other.tree().node(book).unwrap().name().into(),
        kind: stokesforge_app_state::state::WindowKind::Book,
    };
    assert!(!other.project_tree().contains(&entry));
    // #152: "Show Data" of a curve only reveals the workbook of that curve.
    let series = stokesforge_plot::SeriesRef::new(0, 0);
    assert_eq!(
        stokesforge_app_state::state::series_data_books(other.tree(), copied, series),
        books
    );
    assert!(
        stokesforge_app_state::state::series_data_books(
            other.tree(),
            copied,
            stokesforge_plot::SeriesRef::new(0, 99)
        )
        .is_empty(),
        "an absent curve opens no workbook"
    );
    other.dispatch(Action::ShowSeriesData {
        graph: copied,
        series,
    });
    assert!(
        matches!(other.tree().node(book).unwrap().kind(), NodeKind::Workbook { meta, .. } if !meta.hidden)
    );
    assert!(other.project_tree().contains(&entry));
    assert_eq!(other.take_copied_objects()[0].1, book);
    other.dispatch(Action::ShowGraphData { graph: copied });
    assert_eq!(
        other.take_copied_objects()[0].1,
        book,
        "Show Data of the graph stays available"
    );
    other.dispatch(Action::Undo);
    assert!(
        matches!(other.tree().node(book).unwrap().kind(), NodeKind::Workbook { meta, .. } if meta.hidden)
    );
    other.dispatch(Action::Undo);
    assert_eq!(other.tree(), &before);
    other.dispatch(Action::Redo);
    assert!(other.tree().contains(copied));
    let after = other.tree().clone();
    other.dispatch(Action::PasteObjects {
        parent,
        text: "ordinary clipboard".into(),
    });
    assert_eq!(other.tree(), &after);
}

#[test]
fn sheet_rename_passed_to_the_detached_view() {
    let (_session, mut state, book, _, _) = fixture();
    let sheet = state.tree().children(book).unwrap()[0];
    state.dispatch(Action::BeginRenameSheet(sheet));
    let mut view = state.fork_for_detached_view().unwrap();
    assert_eq!(view.renaming_sheet(), Some(sheet));
    state.dispatch(Action::CancelRenameSheet);
    view.sync_detached_sheet_editor(&state);
    assert_eq!(view.renaming_sheet(), None);
}

#[test]
fn renaming_several_objects_is_a_single_gesture_and_a_failure_is_atomic() {
    let (_session, mut state, book, graph, _) = fixture();
    let before = state.tree().clone();
    state.dispatch(Action::RenameObjects(vec![
        (book, "Measurements".into()),
        (graph, "Plot".into()),
    ]));
    assert_eq!(state.tree().node(book).unwrap().name(), "Measurements");
    assert_eq!(state.tree().node(graph).unwrap().name(), "Plot");
    state.dispatch(Action::Undo);
    assert_eq!(state.tree(), &before);
    state.dispatch(Action::RenameObjects(vec![
        (book, "Same".into()),
        (graph, "Same".into()),
    ]));
    assert_eq!(state.tree(), &before);
}

#[test]
fn paste_into_another_folder_and_grouped_move() {
    let mut project = Project::new("Test").unwrap();
    let root = project.root();
    let a = project.add_folder(root, "A").unwrap();
    let b = project.add_folder(root, "B").unwrap();
    let book = project.add_workbook(a, Some("Book1")).unwrap();
    let note = project.add_note(a, Some("Note1"), "Notes").unwrap();
    let session = Session::spawn(project, None).unwrap();
    let mut state = AppState::new_with_preferences(
        session.handle(),
        String::new(),
        stokesforge_app_state::preferences::Preferences::load_from(None),
    )
    .unwrap();
    state.dispatch(Action::MoveObjects {
        ids: vec![book, note],
        parent: b,
    });
    assert_eq!(state.tree().node(book).unwrap().parent(), Some(b));
    state.dispatch(Action::Undo);
    assert_eq!(state.tree().node(book).unwrap().parent(), Some(a));
    state.dispatch(Action::CopyObjects(vec![book, note]));
    let text = state.take_object_clipboard_text().unwrap();
    state.dispatch(Action::PasteObjects { parent: b, text });
    assert_eq!(state.take_copied_objects().len(), 2);
    assert_eq!(state.tree().children(b).unwrap().len(), 2);
    state.dispatch(Action::Undo);
    assert!(state.tree().children(b).unwrap().is_empty());
}

#[test]
fn two_graph_pastes_are_independent_and_frozen() {
    let (_session, mut source, _, graph, y) = fixture();
    source.dispatch(Action::CopyObjects(vec![graph]));
    let text = source.take_object_clipboard_text().unwrap();
    source.dispatch(Action::CommitCell {
        column: y,
        row: 0,
        text: "99".into(),
    });
    let (_session, mut target, _, _, _) = fixture();
    let parent = target.tree().root();
    let before = target.tree().clone();
    let mut graphs = Vec::new();
    for _ in 0..2 {
        target.dispatch(Action::PasteObjects {
            parent,
            text: text.clone(),
        });
        let graph = target.take_copied_objects()[0].1;
        let spec = target.plot_spec(graph).unwrap();
        let column = spec.panels[0].series[0].y;
        assert_eq!(
            target
                .tree()
                .node(column)
                .unwrap()
                .as_column()
                .unwrap()
                .get(0),
            stokesforge_core::Cell::Float(3.)
        );
        graphs.push(graph);
    }
    assert_ne!(
        stokesforge_app_state::state::graph_data_books(target.tree(), graphs[0]),
        stokesforge_app_state::state::graph_data_books(target.tree(), graphs[1])
    );
    target.dispatch(Action::Undo);
    target.dispatch(Action::Undo);
    assert_eq!(target.tree(), &before);
}

#[test]
fn embedded_calculated_columns_keep_their_values_without_dependencies() {
    let (_session, mut source, _, graph, y) = fixture();
    source.dispatch(Action::SetColumnFormula {
        column: y,
        formula: Some("A*2".into()),
    });
    let mut spec = source.plot_spec(graph).unwrap();
    spec.panels.iter_mut().next().unwrap().series[0].x = None; // A is part of the calculation, but is not plotted.
    source.dispatch(Action::SetPlotSpec { graph, spec });
    assert_eq!(
        source.tree().node(y).unwrap().as_column().unwrap().get(0),
        stokesforge_core::Cell::Float(2.)
    );
    source.dispatch(Action::CopyObjects(vec![graph]));
    let text = source.take_object_clipboard_text().unwrap();
    let (_session, mut target, _, _, _) = fixture();
    let parent = target.tree().root();
    target.dispatch(Action::PasteObjects { parent, text });
    let copied = target.take_copied_objects()[0].1;
    let column = target.plot_spec(copied).unwrap().panels[0].series[0].y;
    let data = target.tree().node(column).unwrap().as_column().unwrap();
    assert_eq!(data.get(0), stokesforge_core::Cell::Float(2.));
    assert_eq!(data.meta().formula, None);
}

/// #170, point 4: copying or duplicating a folder is refused for now
/// (the local copy would keep its references to the original workbooks).
/// Nothing is written to the clipboard or the tree, the history does not move,
/// and the status bar says why. Witness: the same copy of a window works.
#[test]
fn copying_or_duplicating_a_folder_is_refused_with_its_reason() {
    let mut project = Project::new("Folders").unwrap();
    let root = project.root();
    let folder = project.add_folder(root, "Folder").unwrap();
    let note = project.add_note(root, Some("Note1"), "").unwrap();
    let session = Session::spawn(project, None).unwrap();
    let mut state = AppState::new_with_preferences(
        session.handle(),
        String::new(),
        stokesforge_app_state::preferences::Preferences::load_from(None),
    )
    .unwrap();
    let before = state.tree().clone();
    let undo = state.snapshot().history.undo_label.clone();

    for action in [
        Action::CopyObjects(vec![folder]),
        Action::CopyObjects(vec![note, folder]),
        Action::DuplicateObjects(vec![folder]),
        Action::DuplicateObjects(vec![note, folder]),
    ] {
        state.dispatch(action.clone());
        assert_eq!(state.take_object_clipboard_text(), None, "{action:?}");
        assert_eq!(state.tree(), &before, "{action:?}");
        assert_eq!(state.snapshot().history.undo_label, undo, "{action:?}");
        let status = state.status().expect("the reason is given");
        assert_eq!(status.level, StatusLevel::Warning, "{action:?}");
        assert_eq!(status.text, tr!("disabled-copy-folder"), "{action:?}");
    }

    state.dispatch(Action::CopyObjects(vec![note]));
    assert!(
        state.take_object_clipboard_text().is_some(),
        "witness: a window can be copied"
    );
}

/// `Book1` holds `Sheet1` (A, B = `max(Sheet2!A)`, C = `max([Book2]Sheet1!A)`)
/// and `Sheet2`; `Book2` holds `Sheet1` (#403). Returns the session, the
/// state, the root and `Book1`.
fn remote_formula_fixture() -> (Session, AppState, NodeId, NodeId) {
    let mut project = Project::new("Test").unwrap();
    let root = project.root();
    let book = project.add_workbook(root, Some("Book1")).unwrap();
    let book2 = project.add_workbook(root, Some("Book2")).unwrap();
    let sheet1 = project.tree().children(book).unwrap()[0];
    let sheet2 = project.add_worksheet(book, Some("Sheet2")).unwrap();
    let other = project.tree().children(book2).unwrap()[0];
    for sheet in [sheet2, other] {
        project
            .add_column(sheet, None, Some("A"), Column::from_f64(&[5., 7.]))
            .unwrap();
    }
    project
        .add_column(sheet1, None, Some("A"), Column::from_f64(&[1., 2.]))
        .unwrap();
    for (name, text) in [("B", "max(Sheet2!A)"), ("C", "max([Book2]Sheet1!A)")] {
        let id = project
            .add_column(sheet1, None, Some(name), Column::from_f64(&[0., 0.]))
            .unwrap();
        let formula = project.tree().formula_from_letters(id, text).unwrap();
        project
            .set_column_meta(
                id,
                stokesforge_core::ColumnMeta {
                    formula: Some(formula),
                    ..Default::default()
                },
            )
            .unwrap();
    }
    let session = Session::spawn(project, None).unwrap();
    let state = AppState::new_with_preferences(
        session.handle(),
        String::new(),
        stokesforge_app_state::preferences::Preferences::load_from(None),
    )
    .unwrap();
    (session, state, root, book)
}

/// The texts of the formulas B and C of the first sheet of a pasted `Book1`,
/// and the sheet `B` reads.
fn pasted_formulas(state: &AppState, pasted_book: NodeId) -> (String, String, Option<NodeId>) {
    let sheets = state.tree().children(pasted_book).unwrap();
    let columns = state.tree().children(sheets[0]).unwrap();
    let read_sheet = state
        .tree()
        .node(columns[1])
        .and_then(|node| node.as_column())
        .and_then(|column| column.meta().formula.as_ref())
        .and_then(|formula| formula.remote_sheets().values().next().copied());
    assert_eq!(read_sheet.is_some(), read_sheet == Some(sheets[1]));
    (
        state.tree().formula_letters(columns[1]).unwrap(),
        state.tree().formula_letters(columns[2]).unwrap(),
        read_sheet,
    )
}

/// #403: the object clipboard goes through the `.stf` conversions. Pasted in
/// the project it was copied from, a workbook keeps its references: to its
/// own copied sheet, and to a sheet outside the copy. Pasted in another
/// project, only the reference to a sheet outside the copy is `#REF` (the other
/// project's identifiers mean nothing there), even when it holds the same names.
#[test]
fn object_clipboard_keeps_remote_references_in_its_project_only() {
    let (_session, mut state, root, book) = remote_formula_fixture();
    state.dispatch(Action::CopyObjects(vec![book]));
    let text = state.take_object_clipboard_text().unwrap();
    state.dispatch(Action::PasteObjects {
        parent: root,
        text: text.clone(),
    });
    let pasted = state.take_copied_objects()[0].1;
    assert_ne!(pasted, book);
    let (inside, outside, read_sheet) = pasted_formulas(&state, pasted);
    assert_eq!(inside, "max(Sheet2!A)");
    assert!(read_sheet.is_some(), "B reads the pasted Sheet2");
    assert_eq!(outside, "max([Book2]Sheet1!A)");

    let (_session2, mut other, other_root, _) = remote_formula_fixture();
    other.dispatch(Action::PasteObjects {
        parent: other_root,
        text,
    });
    let pasted = other.take_copied_objects()[0].1;
    let (inside, outside, _) = pasted_formulas(&other, pasted);
    assert_eq!(inside, "max(Sheet2!A)", "the copied sheet comes along");
    assert_eq!(outside, "max(#REF)");
}

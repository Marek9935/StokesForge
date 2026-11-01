//! #105: Project Manager decisions without a display server.

use stokesforge_app_state::state::{Action, AppState, ProjectTreeNode, UnsavedChoice};
use stokesforge_app_state::tree_drop::MovePlace;
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
fn windows_duplicated_together_read_each_others_copies_in_one_history_entry() {
    let (_session, mut state, book, graph, y) = fixture();
    let before = state.tree().clone();
    // The graph is listed before the workbook it plots, and the book twice.
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
    let sheet_copy = state.tree().children(book_copy).unwrap()[0];
    let y_copy = state.tree().children(sheet_copy).unwrap()[1];
    let NodeKind::Graph { spec, .. } = state.tree().node(graph_copy).unwrap().kind() else {
        panic!("graph")
    };
    assert!(spec.references(y_copy), "the copied graph plots the copy");
    assert!(!spec.references(y), "and no longer the original");
    // Deleting the original workbook reports the original graph only.
    state.dispatch(Action::DeleteObjects(vec![book]));
    let text = state.status().expect("message").text.clone();
    assert!(text.contains("Graph1"), "{text}");
    assert!(!text.contains("Graph1 copy"), "{text}");
    state.dispatch(Action::Undo);
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
        place: MovePlace::Inside,
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

/// `Folder` holds `Book1` (A, B) and `Graph1` (plots B); `Other` is an empty
/// folder. Returns the session, the state and the ids (#391).
struct FolderFixture {
    _session: Session,
    state: AppState,
    root: NodeId,
    folder: NodeId,
    other: NodeId,
    book: NodeId,
}

fn folder_fixture() -> FolderFixture {
    let mut project = Project::new("Folders").unwrap();
    let root = project.root();
    let folder = project.add_folder(root, "Folder").unwrap();
    let other = project.add_folder(root, "Other").unwrap();
    let book = project.add_workbook(folder, Some("Book1")).unwrap();
    let sheet = project.tree().children(book).unwrap()[0];
    project
        .add_column(sheet, None, Some("A"), Column::from_f64(&[1., 2.]))
        .unwrap();
    let b = project
        .add_column(sheet, None, Some("B"), Column::from_f64(&[3., 4.]))
        .unwrap();
    let mut panel = PanelSpec::default();
    panel.series.push(SeriesSpec::new(b));
    project
        .add_graph(
            folder,
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
    FolderFixture {
        _session: session,
        state,
        root,
        folder,
        other,
        book,
    }
}

fn child_by_name(state: &AppState, parent: NodeId, name: &str) -> NodeId {
    *state
        .tree()
        .children(parent)
        .unwrap()
        .iter()
        .find(|id| state.tree().node(**id).unwrap().name() == name)
        .unwrap_or_else(|| panic!("no child {name}"))
}

fn names_in(state: &AppState, parent: NodeId) -> Vec<String> {
    state
        .tree()
        .children(parent)
        .unwrap()
        .iter()
        .map(|id| state.tree().node(*id).unwrap().name().to_string())
        .collect()
}

/// The first column the graph of `folder` plots.
fn plotted_column(state: &AppState, folder: NodeId) -> NodeId {
    let graph = child_by_name(state, folder, "Graph1");
    state.plot_spec(graph).unwrap().panels[0].series[0].y
}

/// #391 (C1): copying a folder and pasting it into another folder gives
/// `<name> copy 1`, inner windows keep their names, and the copied graph
/// plots the copied workbook. Witness: the original graph keeps its own.
#[test]
fn a_copied_folder_pasted_into_another_folder_reads_its_own_workbook() {
    let mut f = folder_fixture();
    let original_column = plotted_column(&f.state, f.folder);
    f.state.dispatch(Action::CopyObjects(vec![f.folder]));
    let text = f.state.take_object_clipboard_text().expect("copied");
    f.state.dispatch(Action::PasteObjects {
        parent: f.other,
        text,
    });
    assert_eq!(names_in(&f.state, f.other), vec!["Folder copy 1"]);
    let copy = child_by_name(&f.state, f.other, "Folder copy 1");
    assert_eq!(names_in(&f.state, copy), vec!["Book1", "Graph1"]);
    let copied_book = child_by_name(&f.state, copy, "Book1");
    assert_ne!(copied_book, f.book);
    let copied_column = plotted_column(&f.state, copy);
    assert_ne!(copied_column, original_column);
    assert_eq!(
        f.state.tree().parent(copied_column).unwrap().and_then(|s| f
            .state
            .tree()
            .parent(s)
            .unwrap()),
        Some(copied_book)
    );
    assert_eq!(plotted_column(&f.state, f.folder), original_column);
}

/// #391 (C2): Ctrl+V with the copied folder itself selected pastes beside it,
/// not inside it. Witness: without the redirect the copy lands in `Folder`.
#[test]
fn pasting_with_the_copied_folder_selected_puts_the_copy_beside_it() {
    let mut f = folder_fixture();
    f.state.dispatch(Action::CopyObjects(vec![f.folder]));
    let text = f.state.take_object_clipboard_text().expect("copied");
    f.state.dispatch(Action::PasteObjects {
        parent: f.folder,
        text,
    });
    assert_eq!(
        names_in(&f.state, f.root),
        vec!["Folder", "Other", "Folder copy 1"]
    );
    assert_eq!(names_in(&f.state, f.folder), vec!["Book1", "Graph1"]);
}

/// #391: Duplicate puts the copy beside the original in one history entry:
/// one Undo restores the tree, one Redo brings the copy back.
#[test]
fn duplicating_a_folder_is_one_history_entry_beside_the_original() {
    let mut f = folder_fixture();
    let before = f.state.tree().clone();
    f.state.dispatch(Action::DuplicateObjects(vec![f.folder]));
    assert_eq!(
        names_in(&f.state, f.root),
        vec!["Folder", "Other", "Folder copy 1"]
    );
    assert_eq!(
        f.state.snapshot().history.undo_label,
        Some(HistoryLabel::Macro(MacroKind::DuplicateObjects))
    );
    let after = f.state.tree().clone();
    f.state.dispatch(Action::Undo);
    assert_eq!(f.state.tree(), &before);
    f.state.dispatch(Action::Redo);
    assert_eq!(f.state.tree(), &after);
}

/// #391: a folder and a window inside it, selected together, give a single
/// copy (the window travels with its folder).
#[test]
fn a_folder_and_a_window_inside_it_make_a_single_copy() {
    let mut f = folder_fixture();
    f.state
        .dispatch(Action::DuplicateObjects(vec![f.folder, f.book]));
    assert_eq!(f.state.take_copied_objects().len(), 1);
    assert_eq!(
        names_in(&f.state, f.root),
        vec!["Folder", "Other", "Folder copy 1"]
    );
    assert_eq!(names_in(&f.state, f.folder), vec!["Book1", "Graph1"]);
}

/// #391 with #306: after copying a folder, deleting the original workbook
/// names only the original graph; the copied one reads the copied workbook.
#[test]
fn the_copied_graph_is_not_a_user_of_the_original_workbook() {
    let mut f = folder_fixture();
    f.state.dispatch(Action::DuplicateObjects(vec![f.folder]));
    assert_eq!(f.state.object_users(&[f.book]), vec!["Graph1"]);
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

// -- Cut and paste = move (#423) ------------------------------------------

#[test]
fn cut_then_paste_of_the_same_text_moves_the_node_in_one_history_entry() {
    let mut f = folder_fixture();
    let (book, other, folder) = (f.book, f.other, f.folder);
    let before = f.state.tree().clone();
    f.state.dispatch(Action::CutObjects(vec![book]));
    assert_eq!(f.state.cut_objects(), vec![book], "the cut rows are marked");
    let text = f
        .state
        .take_object_clipboard_text()
        .expect("cut copies too");
    f.state.dispatch(Action::PasteObjects {
        parent: other,
        text,
    });
    assert_eq!(
        f.state.tree().node(book).unwrap().parent(),
        Some(other),
        "moved, same NodeId"
    );
    assert_eq!(names_in(&f.state, other), vec!["Book1"], "no copy made");
    assert!(f.state.take_copied_objects().is_empty());
    assert!(f.state.cut_objects().is_empty(), "the mark is cleared");
    assert_eq!(
        f.state.snapshot().history.undo_label,
        Some(HistoryLabel::Macro(MacroKind::MoveObjects))
    );
    f.state.dispatch(Action::Undo);
    assert_eq!(f.state.tree(), &before, "one Undo restores");
    assert_eq!(f.state.tree().node(book).unwrap().parent(), Some(folder));
}

#[test]
fn a_cut_pastes_once_a_second_paste_does_nothing_until_a_new_copy() {
    let mut f = folder_fixture();
    f.state.dispatch(Action::CutObjects(vec![f.book]));
    let text = f.state.take_object_clipboard_text().unwrap();
    f.state.dispatch(Action::PasteObjects {
        parent: f.other,
        text: text.clone(),
    });
    let after_move = f.state.tree().clone();
    f.state.dispatch(Action::PasteObjects {
        parent: f.root,
        text: text.clone(),
    });
    assert_eq!(f.state.tree(), &after_move, "the second paste does nothing");
    assert!(f.state.take_copied_objects().is_empty());
    // A new Copy of the same node pastes normally.
    f.state.dispatch(Action::CopyObjects(vec![f.book]));
    let copied = f.state.take_object_clipboard_text().unwrap();
    f.state.dispatch(Action::PasteObjects {
        parent: f.root,
        text: copied,
    });
    assert_eq!(
        f.state.take_copied_objects().len(),
        1,
        "Copy rearms the paste"
    );
    // The consumed text itself pastes again after the Copy replaced it.
    f.state.dispatch(Action::PasteObjects {
        parent: f.root,
        text,
    });
    assert_eq!(f.state.take_copied_objects().len(), 1);
}

#[test]
fn a_paste_of_a_text_from_elsewhere_copies_and_keeps_the_mark() {
    let mut f = folder_fixture();
    let mut outside = folder_fixture();
    outside
        .state
        .dispatch(Action::CopyObjects(vec![outside.book]));
    let foreign = outside.state.take_object_clipboard_text().unwrap();
    f.state.dispatch(Action::CutObjects(vec![f.book]));
    f.state.dispatch(Action::PasteObjects {
        parent: f.other,
        text: foreign,
    });
    assert_eq!(
        f.state.tree().node(f.book).unwrap().parent(),
        Some(f.folder),
        "the cut node did not move"
    );
    assert_eq!(f.state.take_copied_objects().len(), 1, "a copy was pasted");
    assert_eq!(f.state.cut_objects(), vec![f.book], "the mark stays");
    // An older copy of this same project, pasted after the cut: same origin,
    // other text, so it copies as well.
    let graph = child_by_name(&f.state, f.folder, "Graph1");
    f.state.dispatch(Action::CopyObjects(vec![graph]));
    let older = f.state.take_object_clipboard_text().unwrap();
    f.state.dispatch(Action::CutObjects(vec![f.book]));
    f.state.dispatch(Action::PasteObjects {
        parent: f.other,
        text: older,
    });
    assert_eq!(
        f.state.tree().node(f.book).unwrap().parent(),
        Some(f.folder),
        "a text other than the cut's never moves"
    );
}

#[test]
fn a_new_project_forgets_the_cut() {
    let mut f = folder_fixture();
    f.state.dispatch(Action::CutObjects(vec![f.book]));
    f.state.dispatch(Action::NewProject {
        pending_edits: false,
    });
    f.state
        .dispatch(Action::ConfirmUnsaved(UnsavedChoice::Discard));
    assert_eq!(f.state.tree().len(), 1, "witness: the project was replaced");
    assert!(f.state.cut_objects().is_empty());
}

#[test]
fn a_copy_or_a_new_cut_clears_the_mark_and_the_old_text_copies() {
    let mut f = folder_fixture();
    f.state.dispatch(Action::CutObjects(vec![f.book]));
    let cut_text = f.state.take_object_clipboard_text().unwrap();
    f.state.dispatch(Action::CopyObjects(vec![f.book]));
    assert!(f.state.cut_objects().is_empty(), "Copy clears the mark");
    f.state.dispatch(Action::PasteObjects {
        parent: f.other,
        text: cut_text,
    });
    assert_eq!(
        f.state.tree().node(f.book).unwrap().parent(),
        Some(f.folder),
        "without the mark the old cut text only copies"
    );
    assert_eq!(f.state.take_copied_objects().len(), 1);
    let graph = child_by_name(&f.state, f.folder, "Graph1");
    f.state.dispatch(Action::CutObjects(vec![f.book]));
    f.state.dispatch(Action::CutObjects(vec![graph]));
    assert_eq!(f.state.cut_objects(), vec![graph], "a new cut replaces it");
}

#[test]
fn a_cut_node_that_is_deleted_drops_out_of_the_mark() {
    let mut f = folder_fixture();
    f.state.dispatch(Action::CutObjects(vec![f.book]));
    let text = f.state.take_object_clipboard_text().unwrap();
    f.state.dispatch(Action::DeleteObjects(vec![f.book]));
    assert!(f.state.cut_objects().is_empty(), "stale id filtered");
    let before = f.state.tree().clone();
    f.state.dispatch(Action::PasteObjects {
        parent: f.other,
        text,
    });
    assert_eq!(f.state.tree(), &before, "nothing to move, nothing pasted");
}

#[test]
fn a_cut_folder_cannot_go_into_itself_or_its_subfolder() {
    let mut f = folder_fixture();
    f.state.dispatch(Action::AddFolder { parent: f.folder });
    let sub = *f
        .state
        .tree()
        .children(f.folder)
        .unwrap()
        .iter()
        .find(|id| f.state.tree().node(**id).unwrap().kind().is_folder())
        .expect("the new subfolder");
    f.state.dispatch(Action::CutObjects(vec![f.folder]));
    let text = f.state.take_object_clipboard_text().unwrap();
    let before = f.state.tree().clone();
    // Into its own descendant: refused with a sentence, nothing moved.
    f.state.dispatch(Action::PasteObjects {
        parent: sub,
        text: text.clone(),
    });
    assert_eq!(f.state.tree(), &before);
    let status = f.state.status().expect("a message");
    assert!(status.is_error());
    assert!(
        !status.text.to_lowercase().contains("structure"),
        "no raw core detail: {}",
        status.text
    );
    // The cut folder itself selected (beside it, #391): nothing moves and the
    // mark is kept.
    f.state.dispatch(Action::PasteObjects {
        parent: f.folder,
        text,
    });
    assert_eq!(f.state.tree(), &before);
    assert_eq!(f.state.cut_objects(), vec![f.folder]);
}

#[test]
fn a_name_already_taken_in_the_target_refuses_the_whole_move() {
    let mut f = folder_fixture();
    // A "Book1" already in Other.
    f.state.dispatch(Action::CopyObjects(vec![f.book]));
    let copy_text = f.state.take_object_clipboard_text().unwrap();
    f.state.dispatch(Action::PasteObjects {
        parent: f.other,
        text: copy_text,
    });
    let pasted = f.state.take_copied_objects()[0].1;
    f.state
        .dispatch(Action::RenameObjects(vec![(pasted, "Book1".into())]));
    assert_eq!(names_in(&f.state, f.other), vec!["Book1"]);
    let graph = child_by_name(&f.state, f.folder, "Graph1");
    f.state.dispatch(Action::CutObjects(vec![graph, f.book]));
    let text = f.state.take_object_clipboard_text().unwrap();
    let before = f.state.tree().clone();
    f.state.dispatch(Action::PasteObjects {
        parent: f.other,
        text,
    });
    assert_eq!(f.state.tree(), &before, "Graph1 did not move either");
    let status = f.state.status().expect("a message");
    assert!(status.is_error());
    assert!(status.text.contains("Book1"), "{}", status.text);
    assert_eq!(f.state.cut_objects().len(), 2, "the mark is kept");
}

#[test]
fn pasting_a_cut_into_another_project_copies() {
    let mut f = folder_fixture();
    let mut target = folder_fixture();
    f.state.dispatch(Action::CutObjects(vec![f.book]));
    let text = f.state.take_object_clipboard_text().unwrap();
    let parent = target.other;
    target.state.dispatch(Action::PasteObjects { parent, text });
    assert_eq!(target.state.take_copied_objects().len(), 1);
    assert_eq!(
        f.state.tree().node(f.book).unwrap().parent(),
        Some(f.folder),
        "the source stays where it was"
    );
}

// -- drag and drop, folders sorted by default (#425) --

/// Root: Graph10, B/, Book2, Graph2, A/ in model order, so the sorted display is
/// A, B, Book2, Graph2, Graph10. `hidden` is a hidden workbook, `a` and `b` the folders.
struct OrderFixture {
    _session: Session,
    state: AppState,
    root: NodeId,
    a: NodeId,
    b: NodeId,
    book2: NodeId,
    graph2: NodeId,
    graph10: NodeId,
    hidden: NodeId,
}

fn order_fixture() -> OrderFixture {
    let mut project = Project::new("Order").unwrap();
    let root = project.root();
    let graph10 = project.add_workbook(root, Some("Graph10")).unwrap();
    let b = project.add_folder(root, "B").unwrap();
    let book2 = project.add_workbook(root, Some("Book2")).unwrap();
    let graph2 = project.add_workbook(root, Some("Graph2")).unwrap();
    let a = project.add_folder(root, "A").unwrap();
    let hidden = project.add_workbook(root, Some("Hidden")).unwrap();
    let mut meta = match project.tree().node(hidden).unwrap().kind() {
        NodeKind::Workbook { meta, .. } => meta.clone(),
        _ => unreachable!(),
    };
    meta.hidden = true;
    project.set_window_meta(hidden, meta).unwrap();
    let session = Session::spawn(project, None).unwrap();
    let state = AppState::new_with_preferences(
        session.handle(),
        String::new(),
        stokesforge_app_state::preferences::Preferences::load_from(None),
    )
    .unwrap();
    OrderFixture {
        _session: session,
        state,
        root,
        a,
        b,
        book2,
        graph2,
        graph10,
        hidden,
    }
}

fn shown_names(state: &AppState) -> Vec<String> {
    state
        .project_tree()
        .iter()
        .map(|n| match n {
            ProjectTreeNode::Folder { name, .. } | ProjectTreeNode::Window { name, .. } => {
                name.clone()
            }
        })
        .collect()
}

#[test]
fn folders_open_sorted_folders_first_in_natural_order_and_hidden_stays_hidden() {
    let f = order_fixture();
    assert_eq!(
        shown_names(&f.state),
        vec!["A", "B", "Book2", "Graph2", "Graph10"]
    );
    assert!(f.state.tree().node(f.hidden).is_some());
}

#[test]
fn a_drop_inside_a_folder_appends_keeps_it_automatic_and_is_one_entry() {
    let mut f = order_fixture();
    let before = f.state.tree().clone();
    f.state.dispatch(Action::MoveObjects {
        ids: vec![f.graph10, f.book2],
        parent: f.a,
        place: MovePlace::Inside,
    });
    assert_eq!(
        names_in(&f.state, f.a),
        vec!["Book2", "Graph10"],
        "displayed order"
    );
    assert!(!f.state.tree().is_manual_order(f.a));
    assert!(!f.state.tree().is_manual_order(f.root));
    f.state.dispatch(Action::Undo);
    assert_eq!(f.state.tree(), &before, "one Ctrl+Z");
}

#[test]
fn an_insertion_puts_the_folder_in_manual_order_with_the_shown_order_plus_the_row() {
    let mut f = order_fixture();
    let before = f.state.tree().clone();
    // Graph10 dropped between Book2 and Graph2 in the sorted root.
    f.state.dispatch(Action::MoveObjects {
        ids: vec![f.graph10],
        parent: f.root,
        place: MovePlace::Before(f.graph2),
    });
    assert!(f.state.tree().is_manual_order(f.root));
    assert_eq!(
        shown_names(&f.state),
        vec!["A", "B", "Book2", "Graph10", "Graph2"]
    );
    // The model order is the shown order (the hidden workbook keeps its place at the end).
    let model: Vec<NodeId> = f.state.tree().children(f.root).unwrap().to_vec();
    assert_eq!(
        model,
        vec![f.a, f.b, f.book2, f.graph10, f.graph2, f.hidden]
    );
    assert_eq!(
        f.state.snapshot().history.undo_label,
        Some(HistoryLabel::Macro(MacroKind::MoveObjects))
    );
    f.state.dispatch(Action::Undo);
    assert_eq!(f.state.tree(), &before, "flag and order come back together");
    assert!(!f.state.tree().is_manual_order(f.root));
}

#[test]
fn a_drop_that_changes_nothing_leaves_the_folder_automatic_and_the_history_alone() {
    let mut f = order_fixture();
    // Book2 right before Graph2 is where it already is.
    f.state.dispatch(Action::MoveObjects {
        ids: vec![f.book2],
        parent: f.root,
        place: MovePlace::Before(f.graph2),
    });
    assert!(!f.state.tree().is_manual_order(f.root));
    assert_eq!(f.state.snapshot().history.undo_label, None);
}

#[test]
fn an_insertion_across_folders_moves_the_row_and_reorders() {
    let mut f = order_fixture();
    f.state.dispatch(Action::MoveObjects {
        ids: vec![f.graph10],
        parent: f.a,
        place: MovePlace::AtEnd,
    });
    assert_eq!(f.state.tree().node(f.graph10).unwrap().parent(), Some(f.a));
    assert!(f.state.tree().is_manual_order(f.a));
    assert!(!f.state.tree().is_manual_order(f.root));
}

#[test]
fn a_drop_follows_the_refusals_of_cut_and_paste() {
    let mut f = order_fixture();
    f.state.dispatch(Action::AddFolder { parent: f.a });
    let sub = *f
        .state
        .tree()
        .children(f.a)
        .unwrap()
        .iter()
        .find(|id| f.state.tree().node(**id).unwrap().kind().is_folder())
        .expect("subfolder");
    let before = f.state.tree().clone();
    // A folder into its own descendant: refused with a sentence.
    f.state.dispatch(Action::MoveObjects {
        ids: vec![f.a],
        parent: sub,
        place: MovePlace::Inside,
    });
    assert_eq!(f.state.tree(), &before);
    let status = f.state.status().expect("a message").clone();
    assert!(status.is_error(), "{}", status.text);
    assert_eq!(
        status.text,
        stokesforge_app_state::tr!("error-move-into-itself"),
        "the sentence of Paste"
    );
    // Insertion into it: the same refusal.
    f.state.dispatch(Action::MoveObjects {
        ids: vec![f.a],
        parent: sub,
        place: MovePlace::AtEnd,
    });
    assert_eq!(f.state.tree(), &before);
    // Onto itself: silent.
    f.state.dispatch(Action::MoveObjects {
        ids: vec![f.a],
        parent: f.a,
        place: MovePlace::Inside,
    });
    assert_eq!(f.state.tree(), &before);
    // A name taken in the destination: refused whole, with the name.
    let mut other = order_fixture();
    other.state.dispatch(Action::CopyObjects(vec![other.book2]));
    let text = other.state.take_object_clipboard_text().unwrap();
    other.state.dispatch(Action::PasteObjects {
        parent: other.a,
        text,
    });
    let pasted = other.state.take_copied_objects()[0].1;
    other
        .state
        .dispatch(Action::RenameObjects(vec![(pasted, "Book2".into())]));
    let before = other.state.tree().clone();
    other.state.dispatch(Action::MoveObjects {
        ids: vec![other.graph10, other.book2],
        parent: other.a,
        place: MovePlace::Inside,
    });
    assert_eq!(other.state.tree(), &before, "Graph10 did not move either");
    let status = other.state.status().expect("a message");
    assert!(status.is_error());
    assert!(status.text.contains("Book2"), "{}", status.text);
}

#[test]
fn sort_puts_a_manual_folder_back_in_automatic_order_without_touching_the_model() {
    let mut f = order_fixture();
    f.state.dispatch(Action::MoveObjects {
        ids: vec![f.graph10],
        parent: f.root,
        place: MovePlace::Before(f.a),
    });
    assert_eq!(shown_names(&f.state)[0], "Graph10");
    let model = f.state.tree().children(f.root).unwrap().to_vec();
    f.state.dispatch(Action::SortFolder(f.root));
    assert!(!f.state.tree().is_manual_order(f.root));
    assert_eq!(
        shown_names(&f.state),
        vec!["A", "B", "Book2", "Graph2", "Graph10"]
    );
    assert_eq!(f.state.tree().children(f.root).unwrap(), &model[..]);
    assert_eq!(
        f.state.snapshot().history.undo_label,
        Some(HistoryLabel::SortFolder {
            name: Some("Order".into())
        })
    );
    f.state.dispatch(Action::Undo);
    assert!(f.state.tree().is_manual_order(f.root));
    // Sorting an automatic folder does nothing.
    f.state.dispatch(Action::SortFolder(f.root));
    f.state.dispatch(Action::SortFolder(f.root));
    assert!(!f.state.tree().is_manual_order(f.root));
}

#[test]
fn new_objects_arrive_last_in_a_manual_folder_and_in_place_in_an_automatic_one() {
    let mut f = order_fixture();
    f.state.dispatch(Action::AddWorkbook);
    let new = *f.state.tree().children(f.root).unwrap().last().unwrap();
    // Automatic: the new workbook shows at its sorted place, not last.
    let shown = shown_names(&f.state);
    let name = f.state.tree().node(new).unwrap().name().to_string();
    assert!(
        shown.iter().position(|n| *n == name) < Some(shown.len() - 1),
        "{shown:?}"
    );
    // A rename moves it to its place.
    f.state
        .dispatch(Action::RenameObjects(vec![(new, "Zzz".into())]));
    assert_eq!(shown_names(&f.state).last().unwrap(), "Zzz");
    f.state
        .dispatch(Action::RenameObjects(vec![(new, "Aaa".into())]));
    assert_eq!(shown_names(&f.state)[2], "Aaa");
    // Manual: a new workbook is shown last.
    f.state.dispatch(Action::MoveObjects {
        ids: vec![f.graph10],
        parent: f.root,
        place: MovePlace::Before(f.a),
    });
    f.state.dispatch(Action::AddWorkbook);
    let last = *f.state.tree().children(f.root).unwrap().last().unwrap();
    let last_name = f.state.tree().node(last).unwrap().name().to_string();
    assert_eq!(shown_names(&f.state).last().unwrap(), &last_name);
}

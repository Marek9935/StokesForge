//! #425: a new window is created in the folder of the Project Manager selection.

use std::sync::Arc;
use stokesforge_app_state::creation_folder::creation_folder;
use stokesforge_app_state::state::{Action, AppState};
use stokesforge_core::{NodeId, Project, Tree};
use stokesforge_session::Session;

struct World {
    tree: Arc<Tree>,
    root: NodeId,
    folder: NodeId,
    book_in_folder: NodeId,
    book_at_root: NodeId,
}

fn world() -> (Project, World) {
    let mut project = Project::new("Test").unwrap();
    let root = project.root();
    let folder = project.add_folder(root, "Folder").unwrap();
    let book_in_folder = project.add_workbook(folder, Some("Inside")).unwrap();
    let book_at_root = project.add_workbook(root, Some("Outside")).unwrap();
    let world = World {
        tree: project.tree().clone(),
        root,
        folder,
        book_in_folder,
        book_at_root,
    };
    (project, world)
}

#[test]
fn the_creation_folder_follows_the_selection() {
    let (_project, w) = world();
    let pick = |selection: &[NodeId]| creation_folder(&w.tree, selection);
    assert_eq!(pick(&[w.folder]), w.folder, "a folder: inside it");
    assert_eq!(pick(&[w.book_in_folder]), w.folder, "a window: its folder");
    assert_eq!(pick(&[w.book_at_root]), w.root, "a root window: the root");
    assert_eq!(pick(&[]), w.root, "nothing: the root");
    assert_eq!(
        pick(&[w.folder, w.book_at_root]),
        w.root,
        "several rows: the root"
    );
    assert_eq!(
        pick(&[NodeId::from_raw(987_654)]),
        w.root,
        "an unknown node: the root"
    );
}

fn state_with(selection: impl Fn(&World) -> Vec<NodeId>) -> (Session, AppState, World) {
    let (project, w) = world();
    let session = Session::spawn(project, None).unwrap();
    let mut state = AppState::new_with_preferences(
        session.handle(),
        String::new(),
        stokesforge_app_state::preferences::Preferences::load_from(None),
    )
    .unwrap();
    let selected = selection(&w);
    state.set_project_manager_selection(selected);
    (session, state, w)
}

fn children(state: &AppState, folder: NodeId) -> Vec<NodeId> {
    state.tree().children(folder).unwrap().to_vec()
}

#[test]
fn a_new_workbook_lands_in_the_selected_folder_and_one_undo_removes_it() {
    let (_session, mut state, w) = state_with(|w| vec![w.folder]);
    let (in_folder, at_root) = (children(&state, w.folder), children(&state, w.root));
    state.dispatch(Action::AddWorkbook);
    assert_eq!(children(&state, w.folder).len(), in_folder.len() + 1);
    assert_eq!(children(&state, w.root), at_root, "nothing at the root");
    state.dispatch(Action::Undo);
    assert_eq!(children(&state, w.folder), in_folder, "one undo");
    assert_eq!(children(&state, w.root), at_root);
}

#[test]
fn a_new_graph_lands_in_the_folder_of_the_selected_window() {
    let (_session, mut state, w) = state_with(|w| vec![w.book_in_folder]);
    let (in_folder, at_root) = (children(&state, w.folder), children(&state, w.root));
    state.dispatch(Action::AddGraph);
    assert_eq!(children(&state, w.folder).len(), in_folder.len() + 1);
    assert_eq!(children(&state, w.root), at_root);
    state.dispatch(Action::Undo);
    assert_eq!(children(&state, w.folder), in_folder, "one undo");
}

#[test]
fn with_two_rows_selected_a_new_window_goes_to_the_root() {
    let (_session, mut state, w) = state_with(|w| vec![w.folder, w.book_at_root]);
    let in_folder = children(&state, w.folder);
    let at_root = children(&state, w.root);
    state.dispatch(Action::AddWorkbook);
    state.dispatch(Action::AddGraph);
    assert_eq!(children(&state, w.folder), in_folder);
    assert_eq!(children(&state, w.root).len(), at_root.len() + 2);
}

// -- #437: automatic names skip sibling folders --

struct FolderWorld {
    root: NodeId,
    folder: NodeId,
}

/// Root holding folders `Book1`, `Graph1` and `F`; `F` holds a folder `Graph1`.
fn folder_world() -> (Session, AppState, FolderWorld) {
    let mut project = Project::new("Test").unwrap();
    let root = project.root();
    project.add_folder(root, "Book1").unwrap();
    project.add_folder(root, "Graph1").unwrap();
    let folder = project.add_folder(root, "F").unwrap();
    project.add_folder(folder, "Graph1").unwrap();
    let session = Session::spawn(project, None).unwrap();
    let state = AppState::new_with_preferences(
        session.handle(),
        String::new(),
        stokesforge_app_state::preferences::Preferences::load_from(None),
    )
    .unwrap();
    (session, state, FolderWorld { root, folder })
}

fn name_of(state: &AppState, id: NodeId) -> String {
    state.tree().node(id).unwrap().name().to_string()
}

fn new_child(state: &AppState, parent: NodeId, before: &[NodeId]) -> NodeId {
    let now = children(state, parent);
    assert_eq!(now.len(), before.len() + 1, "exactly one new child");
    *now.iter().find(|id| !before.contains(id)).unwrap()
}

#[test]
fn an_unnamed_workbook_skips_a_sibling_folder_without_error() {
    let (_session, mut state, w) = folder_world();
    let before = children(&state, w.root);
    state.dispatch(Action::AddWorkbook);
    let book = new_child(&state, w.root, &before);
    assert_eq!(name_of(&state, book), "Book2");
    assert!(
        !state.status().is_some_and(|m| m.is_error()),
        "no error shown"
    );
    let sheet = state.tree().children(book).unwrap()[0];
    assert_eq!(state.tree().children(sheet).unwrap().len(), 2, "X and Y");
    state.dispatch(Action::Undo);
    assert_eq!(children(&state, w.root), before, "one undo");
}

#[test]
fn an_unnamed_graph_skips_a_sibling_folder_without_error() {
    let (_session, mut state, w) = folder_world();
    let before = children(&state, w.root);
    state.dispatch(Action::AddGraph);
    let graph = new_child(&state, w.root, &before);
    assert_eq!(name_of(&state, graph), "Graph2");
    assert!(
        !state.status().is_some_and(|m| m.is_error()),
        "no error shown"
    );
}

#[test]
fn an_unnamed_graph_in_the_selected_folder_skips_the_folder_it_holds() {
    let (_session, mut state, w) = folder_world();
    state.set_project_manager_selection(vec![w.folder]);
    let before = children(&state, w.folder);
    state.dispatch(Action::AddGraph);
    let graph = new_child(&state, w.folder, &before);
    assert_eq!(name_of(&state, graph), "Graph2");
    assert!(
        !state.status().is_some_and(|m| m.is_error()),
        "no error shown"
    );
}

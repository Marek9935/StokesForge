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

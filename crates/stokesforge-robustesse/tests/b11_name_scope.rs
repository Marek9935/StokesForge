//! Battery 11 — the scope of name uniqueness, on both sides of the tree.
//!
//! A folder is the only parent in the model that accepts two **categories** of
//! children (`NodeKind::accepts_child`: folders *and* windows). The question
//! "is this name free here?" therefore has two possible answers, and three
//! places in the repository answer it:
//!
//! - `Tree::check_name_available` — at mutation time;
//! - `Tree::validate_loaded_names` — when a file is loaded;
//! - `Tree::child_by_name`, filtered by `is_folder` — when a folder path is
//!   resolved.
//!
//! This battery checks that they say the same thing. Since #105, the scope is
//! the parent folder, and ambiguous short paths are refused.

use stokesforge_core::{CoreError, Node, NodeId, NodeKind, Project, Tree, WindowMeta};

fn folder_kind(children: Vec<NodeId>) -> NodeKind {
    NodeKind::Folder { children }
}

fn note_kind() -> NodeKind {
    NodeKind::Note {
        meta: WindowMeta::default(),
        text: String::new(),
    }
}

/// Flat tree: a root and its children, in the given order.
fn tree_of(children: Vec<(u64, &str, NodeKind)>) -> Result<Tree, CoreError> {
    let ids: Vec<NodeId> = children
        .iter()
        .map(|(id, _, _)| NodeId::from_raw(*id))
        .collect();
    let mut nodes = vec![Node::new(NodeId::from_raw(1), "R", folder_kind(ids))?];
    for (id, name, kind) in children {
        nodes.push(Node::with_parent(
            NodeId::from_raw(id),
            name,
            Some(NodeId::from_raw(1)),
            kind,
        )?);
    }
    Tree::from_nodes(NodeId::from_raw(1), nodes)
}

// -- guards ---------------------------------------------------------------

#[test]
fn b11_two_sibling_folders_with_the_same_name_are_rejected() {
    let mut project = Project::new("R").expect("project");
    let root = project.root();
    project.add_folder(root, "X").expect("first folder");
    assert!(
        matches!(
            project.add_folder(root, "X"),
            Err(CoreError::DuplicateName(_))
        ),
        "two sibling folders cannot have the same name"
    );
}

#[test]
fn b11_same_names_in_different_folders_are_accepted_and_canonical_paths_are_unambiguous() {
    let mut project = Project::new("R").expect("project");
    let root = project.root();
    let elsewhere = project.add_folder(root, "Elsewhere").expect("folder");
    let first = project.add_note(root, Some("N"), "root").unwrap();
    let second = project.add_note(elsewhere, Some("N"), "elsewhere").unwrap();
    assert!(matches!(
        project.add_note(root, Some("N"), ""),
        Err(CoreError::DuplicateName(_))
    ));
    assert!(matches!(
        project.add_note(elsewhere, Some("N"), ""),
        Err(CoreError::DuplicateName(_))
    ));
    assert!(
        project.resolve_path("[N]").is_err(),
        "the short name does not arbitrarily pick a note"
    );
    assert_eq!(project.resolve_path("/[N]").unwrap(), first);
    assert_eq!(project.resolve_path("/Elsewhere/[N]").unwrap(), second);
    for id in [first, second] {
        assert_eq!(
            project.resolve_path(&project.path_of(id).unwrap()).unwrap(),
            id
        );
    }
    let before = project.tree().clone();
    assert!(
        project.move_node(second, root, 0).is_err(),
        "a move does not create sibling namesakes"
    );
    assert_eq!(project.tree(), &before);
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("homonyms.stf");
    stokesforge_persistence::save(&project, &path).unwrap();
    let loaded = stokesforge_persistence::load(&path).unwrap();
    assert_eq!(loaded.tree(), &before);
    for id in [first, second] {
        assert_eq!(
            loaded.resolve_path(&loaded.path_of(id).unwrap()).unwrap(),
            id
        );
    }
}

#[test]
fn b11_a_folder_cannot_take_the_name_of_a_sibling_window() {
    // This direction is well guarded: `check_name_available` compares a folder
    // with ALL its siblings, whatever their category. It is the other
    // direction that is not (see reproduction A8 below).
    let mut project = Project::new("R").expect("project");
    let root = project.root();
    project.add_note(root, Some("X"), "").expect("note");
    assert!(
        matches!(
            project.add_folder(root, "X"),
            Err(CoreError::DuplicateName(_))
        ),
        "a folder cannot take the name of a sibling note"
    );
}

#[test]
fn b11_the_loader_rejects_two_windows_with_the_same_name() {
    let error = tree_of(vec![(2, "X", note_kind()), (3, "X", note_kind())])
        .expect_err("two notes with the same name must be rejected");
    assert!(matches!(error, CoreError::Structure(_)), "{error:?}");
}

// -- reproductions --------------------------------------------------------

#[test]
fn b11_a_window_does_not_take_the_name_of_a_sibling_folder() {
    let mut project = Project::new("R").expect("project");
    let root = project.root();
    project.add_folder(root, "X").expect("folder X");

    let note = project.add_note(root, Some("X"), "text");
    println!("add_note(\"X\") next to the folder \"X\" → {note:?}");
    assert!(
        matches!(note, Err(CoreError::DuplicateName(_))),
        "the creation order should change nothing: folder then note is accepted, \
         note then folder is rejected (see the guard b11_a_folder_cannot_take_the_name_of_…)"
    );
}

#[test]
fn b11_path_of_always_resolves_back_to_the_same_node() {
    // A8 (`guide-de-correction-robustesse.md` §F9) — fixed at the root:
    // `check_name_available` now refuses that a window takes the name of a
    // sibling folder (see `b11_a_window_does_not_take_the_name_of_a_sibling_
    // folder`), so the scenario this test used to set up — "note X" accepted
    // next to a "folder X" — can no longer be built at all through the
    // mutation API. The property `path_of ∘ resolve_path = identity`
    // (`b8_properties`) therefore no longer needs to be defended after the
    // fact against this specific case: it became impossible to reach, which
    // this test checks directly.
    let mut project = Project::new("R").expect("project");
    let root = project.root();
    let folder = project.add_folder(root, "X").expect("folder X");
    assert!(
        matches!(
            project.add_note(root, Some("X"), "text"),
            Err(CoreError::DuplicateName(_))
        ),
        "a note must no longer be able to take the name of a sibling folder"
    );

    // The folder itself stays resolvable as usual.
    assert_eq!(project.resolve_path("/X"), Ok(folder));
    let path = project.path_of(folder).expect("folder path");
    assert_eq!(
        project.resolve_path(&path),
        Ok(folder),
        "the canonical path of a node must always designate that node again"
    );
}

#[test]
fn b11_the_loader_rejects_what_the_mutation_api_rejects() {
    // A note then a folder with the same name: `Project::add_folder` rejects
    // exactly this pair (guard b11_a_folder_cannot_take_the_name_of_…), the
    // loader accepts it.
    let result = tree_of(vec![
        (2, "X", note_kind()),
        (3, "X", folder_kind(Vec::new())),
    ]);
    match &result {
        Ok(tree) => println!(
            "loaded: {} nodes, resolve(\"/X\") = {:?}",
            tree.len(),
            tree.resolve("/X")
        ),
        Err(e) => println!("rejected: {e}"),
    }
    assert!(
        result.is_err(),
        "the loader accepts a tree that the mutation API cannot build: the two sets \
         of invariants must coincide, otherwise a file opens a project that no \
         command can reproduce or repair"
    );
}

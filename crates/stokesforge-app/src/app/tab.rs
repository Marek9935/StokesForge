//! The [`Tab`] type: the designation of an open content window (workbook,
//! graph, matrix, note) by its node, its two translations to the model
//! ([`tab_for_window`], [`tab_node`]), its title ([`tab_title`],
//! [`tab_title_from_tree`]) and its mirror readable by `stokesforge_app_state::state`
//! ([`active_tab_kind`]).
//!
//! A module of its own (#117): the window building block (`window_chrome`), the
//! stack (`window_stack`), detaching (`window_detach`) and `app.rs` share it,
//! and the building block must not depend on the stack.

use stokesforge_core::{NodeId, Tree};

use stokesforge_app_state::state::{node_title_in, ActiveTabKind, AppState, WindowKind};
use stokesforge_app_state::tr;

/// A tab, that is, an open content window.
///
/// Neither the Project Manager nor the script console is one — see the
/// module's top doc, §§ Project Manager and Script console: they are two fixed
/// `egui::Panel`s, painted respectively by
/// [`StokesForgeApp::project_manager_panel`](crate::app::StokesForgeApp::project_manager_panel) and [`StokesForgeApp::script_panel`](crate::app::StokesForgeApp::script_panel).
/// [`Tab::Book`], [`Tab::Graph`], [`Tab::Matrix`], [`Tab::Note`] carry a
/// [`NodeId`]: one instance per **open** window, focused rather than
/// duplicated if reopened — see [`StokesForgeApp::open_tab`](crate::app::StokesForgeApp::open_tab).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(super) enum Tab {
    /// An open workbook, identified by its node. Holds the grid,
    /// scoped to the sheet currently chosen *in this workbook*.
    Book(NodeId),
    /// An open graph, identified by its node.
    Graph(NodeId),
    /// An open matrix, identified by its node. **Placeholder** content —
    /// see [`matrix_contents`](crate::app::contents::matrix_contents).
    Matrix(NodeId),
    /// An open note, identified by its node.
    Note(NodeId),
}

/// Translates a Project Manager window kind into the tab that opens it.
pub(super) fn tab_for_window(id: NodeId, kind: WindowKind) -> Tab {
    match kind {
        WindowKind::Book => Tab::Book(id),
        WindowKind::Graph => Tab::Graph(id),
        WindowKind::Matrix => Tab::Matrix(id),
        WindowKind::Note => Tab::Note(id),
    }
}

/// Every variant of [`Tab`] matches a persisted window identified by its
/// node.
pub(super) fn tab_node(tab: Tab) -> NodeId {
    match tab {
        Tab::Book(id) | Tab::Graph(id) | Tab::Matrix(id) | Tab::Note(id) => id,
    }
}

/// Title of a tab: the name of the node it carries, or an explicit "deleted"
/// text if it vanished from the tree in the meantime (closed from another
/// tab, or by a script) — a tab left open on a vanished node must never
/// panic nor disappear silently (invariant 10).
pub(super) fn tab_title(state: &AppState, tab: Tab) -> String {
    tab_title_from_tree(state.tree(), tab)
}

/// [`tab_title`], from a `Tree` already at hand rather than a live
/// `&AppState` — for the header painted by a detached window, which only has
/// the [`Snapshot`](stokesforge_session::Snapshot) resynchronized in `DetachedTabViewModel`
/// (see `window_detach.rs`, § "reading the model is factored out").
pub(super) fn tab_title_from_tree(tree: &Tree, tab: Tab) -> String {
    match tab {
        Tab::Book(id) => node_title_in(tree, id, || tr!("window-kind-workbook")),
        Tab::Graph(id) => node_title_in(tree, id, || tr!("window-kind-graph")),
        Tab::Matrix(id) => node_title_in(tree, id, || tr!("window-kind-matrix")),
        Tab::Note(id) => node_title_in(tree, id, || tr!("window-kind-note")),
    }
}

/// Translates the [`Tab`] that `StokesForgeApp::ui` sees into
/// [`ActiveTabKind`], the public mirror that `stokesforge_app_state::state` can read without
/// knowing `Tab` — the only point of contact between the two, see the top
/// doc of [`ActiveTabKind`].
pub(super) fn active_tab_kind(tab: Option<Tab>) -> ActiveTabKind {
    match tab {
        Some(Tab::Book(id)) => ActiveTabKind::Book(id),
        Some(Tab::Graph(id)) => ActiveTabKind::Graph(id),
        Some(Tab::Matrix(id)) => ActiveTabKind::Matrix(id),
        Some(Tab::Note(id)) => ActiveTabKind::Note(id),
        None => ActiveTabKind::None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use stokesforge_core::Project;
    use stokesforge_plot::PlotSpec;
    use stokesforge_session::Session;

    const KINDS: [WindowKind; 4] = [
        WindowKind::Book,
        WindowKind::Graph,
        WindowKind::Matrix,
        WindowKind::Note,
    ];

    /// Each window kind opens in its own tab, which does designate the node
    /// it comes from; the same node under two kinds gives two distinct tabs
    /// (collapse and the stack track them separately).
    #[test]
    fn a_window_tab_keeps_its_kind_and_its_node() {
        let id = NodeId::from_raw(7);
        let tabs: Vec<Tab> = KINDS.iter().map(|&kind| tab_for_window(id, kind)).collect();
        assert_eq!(
            tabs,
            vec![
                Tab::Book(id),
                Tab::Graph(id),
                Tab::Matrix(id),
                Tab::Note(id)
            ]
        );
        for tab in &tabs {
            assert_eq!(tab_node(*tab), id, "{tab:?}");
        }
        let distinct: std::collections::HashSet<Tab> = tabs.iter().copied().collect();
        assert_eq!(distinct.len(), KINDS.len());
        // The witness: another node gives another tab, of the same kind.
        assert_ne!(
            tab_for_window(NodeId::from_raw(8), WindowKind::Graph),
            Tab::Graph(id)
        );
    }

    /// A project holding one window of each kind, plus one of each kind
    /// added then removed from the tree: the tabs left open on the latter
    /// designate a vanished node. Returns the live state (and its session,
    /// to keep alive), the four present tabs and the four vanished ones, in
    /// the order Book, Graph, Matrix, Note.
    fn fixture() -> (Session, AppState, [Tab; 4], [Tab; 4]) {
        let mut project = Project::new("Titles").unwrap();
        let root = project.root();
        let windows = |project: &mut Project, suffix: &str| {
            [
                Tab::Book(
                    project
                        .add_empty_workbook(root, Some(&format!("Book{suffix}")))
                        .unwrap(),
                ),
                Tab::Graph(
                    project
                        .add_graph(root, Some(&format!("Curve{suffix}")), PlotSpec::default())
                        .unwrap(),
                ),
                Tab::Matrix(
                    project
                        .add_matrix(root, Some(&format!("Map{suffix}")))
                        .unwrap(),
                ),
                Tab::Note(
                    project
                        .add_note(root, Some(&format!("Journal{suffix}")), "")
                        .unwrap(),
                ),
            ]
        };
        let present = windows(&mut project, "");
        let vanished = windows(&mut project, "Deleted");
        for tab in vanished {
            project.remove(tab_node(tab)).unwrap();
        }
        let session = Session::spawn(project, None).unwrap();
        let state = AppState::new(session.handle(), String::new()).unwrap();
        (session, state, present, vanished)
    }

    /// A tab on a live node carries the node's name, whatever its kind, and
    /// the read on a `Tree` already at hand (detached window) says exactly
    /// the same thing as the one on the live state.
    #[test]
    fn a_tab_title_is_the_name_of_its_node() {
        let (_session, state, present, _) = fixture();
        let expected_names = ["Book", "Curve", "Map", "Journal"];
        for (tab, expected) in present.into_iter().zip(expected_names) {
            assert_eq!(tab_title(&state, tab), expected, "{tab:?}");
            assert_eq!(tab_title_from_tree(state.tree(), tab), expected, "{tab:?}");
        }
    }

    /// Invariant 10: a tab left open on a vanished node does not panic
    /// and does not go blank; it says the kind of the lost window, its
    /// identifier and that it was deleted. The witness: the four titles
    /// are distinct, each naming **its** kind — a kind mistaken for another
    /// would show.
    #[test]
    fn a_tab_title_on_a_vanished_node_says_deleted() {
        let (_session, state, _, vanished) = fixture();
        let kinds = [
            tr!("window-kind-workbook"),
            tr!("window-kind-graph"),
            tr!("window-kind-matrix"),
            tr!("window-kind-note"),
        ];
        let mut titles = Vec::new();
        for (tab, kind) in vanished.into_iter().zip(kinds) {
            let id = tab_node(tab);
            assert!(
                state.tree().node(id).is_none(),
                "{tab:?} must have vanished"
            );
            let expected = tr!("window-deleted", kind = kind, id = id.to_string());
            let title = tab_title(&state, tab);
            assert_eq!(title, expected, "{tab:?}");
            assert_eq!(tab_title_from_tree(state.tree(), tab), expected, "{tab:?}");
            assert!(!title.trim().is_empty(), "{tab:?}: empty title");
            assert!(title.contains(&id.to_string()), "{tab:?}: {title:?}");
            titles.push(title);
        }
        let distinct: std::collections::HashSet<&String> = titles.iter().collect();
        assert_eq!(distinct.len(), titles.len(), "{titles:?}");
    }

    /// `stokesforge_app_state::state` reads the active tab through its mirror: each kind
    /// keeps its variant and its node there, and the absence of a tab stays
    /// an absence. The same `NodeId` everywhere: only the kind tells the
    /// variants apart, a kind mistaken for another would show.
    #[test]
    fn the_active_tab_mirror_keeps_kind_and_node() {
        let id = NodeId::from_raw(7);
        let pairs = [
            (Some(Tab::Book(id)), ActiveTabKind::Book(id)),
            (Some(Tab::Graph(id)), ActiveTabKind::Graph(id)),
            (Some(Tab::Matrix(id)), ActiveTabKind::Matrix(id)),
            (Some(Tab::Note(id)), ActiveTabKind::Note(id)),
            (None, ActiveTabKind::None),
        ];
        for (tab, expected) in pairs {
            assert_eq!(active_tab_kind(tab), expected, "{tab:?}");
        }
        let mirrors: std::collections::HashSet<ActiveTabKind> =
            pairs.iter().map(|(tab, _)| active_tab_kind(*tab)).collect();
        assert_eq!(mirrors.len(), pairs.len(), "{mirrors:?}");
        // The node follows: another node gives another mirror.
        assert_eq!(
            active_tab_kind(Some(Tab::Graph(NodeId::from_raw(8)))),
            ActiveTabKind::Graph(NodeId::from_raw(8))
        );
    }
}

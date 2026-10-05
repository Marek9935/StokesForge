//! #105 : décisions du Project Manager sans serveur d'affichage.

use stokesforge_app::state::{Action, AppState};
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
    let state = AppState::new(session.handle(), String::new()).unwrap();
    (session, state, book, graph, y)
}

#[test]
fn duplication_multiple_groupee_et_graphes_sur_les_colonnes_originales() {
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
        panic!("graphe")
    };
    assert!(spec.references(y));
    state.dispatch(Action::Undo);
    assert_eq!(state.tree(), &before);
    state.dispatch(Action::Redo);
    assert!(state.tree().contains(book_copy));
    assert!(state.tree().contains(graph_copy));
}

#[test]
fn presse_papiers_fige_et_collage_entre_projets_avec_identifiants_de_destination() {
    let (_source, mut source, book, _, y) = fixture();
    source.dispatch(Action::CopyObjects(vec![book]));
    let text = source.take_object_clipboard_text().unwrap();
    source.dispatch(Action::CommitCell {
        column: y,
        row: 0,
        text: "99".into(),
    });
    // Les IDs source se recouvrent volontairement avec ceux du destinataire.
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

#[test]
fn suppression_confirmee_nomme_les_graphes_et_un_seul_undo_restaure_tout() {
    let (_session, mut state, book, graph, _) = fixture();
    let before = state.tree().clone();
    state.dispatch(Action::DeleteObjects(vec![book]));
    assert_eq!(state.pending_object_delete(), &[book]);
    assert_eq!(state.object_users(&[book]), vec!["Graph1"]);
    assert_eq!(state.tree(), &before);
    state.dispatch(Action::ConfirmDeleteObjects(false));
    assert_eq!(state.tree(), &before);
    state.dispatch(Action::DeleteObjects(vec![book, graph]));
    state.dispatch(Action::ConfirmDeleteObjects(true));
    assert!(!state.tree().contains(book));
    assert!(!state.tree().contains(graph));
    state.dispatch(Action::Undo);
    assert_eq!(state.tree(), &before);
    state.dispatch(Action::DeleteObjects(vec![graph]));
    assert!(state.pending_object_delete().is_empty());
    assert!(!state.tree().contains(graph));
}

#[test]
fn renommage_local_et_chemins_non_ambigus_apres_enregistrement() {
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
fn graphe_collable_dans_son_projet_et_refuse_dans_un_autre() {
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
    assert_eq!(other.tree(), &before);
    assert!(other.status().unwrap().is_error);
    other.dispatch(Action::PasteObjects {
        parent,
        text: "ordinary clipboard".into(),
    });
    assert_eq!(other.tree(), &before);
}

#[test]
fn renommage_de_feuille_transmis_a_la_vue_detachee() {
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
fn renommer_plusieurs_objets_est_un_seul_geste_et_un_echec_est_atomique() {
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
fn collage_dans_un_autre_dossier_et_deplacement_groupes() {
    let mut project = Project::new("Test").unwrap();
    let root = project.root();
    let a = project.add_folder(root, "A").unwrap();
    let b = project.add_folder(root, "B").unwrap();
    let book = project.add_workbook(a, Some("Book1")).unwrap();
    let note = project.add_note(a, Some("Note1"), "Notes").unwrap();
    let session = Session::spawn(project, None).unwrap();
    let mut state = AppState::new(session.handle(), String::new()).unwrap();
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

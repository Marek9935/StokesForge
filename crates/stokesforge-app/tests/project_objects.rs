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
    let state = AppState::new_with_preferences(
        session.handle(),
        String::new(),
        stokesforge_app::preferences::Preferences::load_from(None),
    )
    .unwrap();
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

/// F3 (#229) : une suppression annulable ne demande rien ; le message nomme
/// les graphes **restants** qui en utilisaient les données, et un seul Undo
/// rend tout.
#[test]
fn suppression_sans_confirmation_nomme_les_graphes_et_un_seul_undo_restaure_tout() {
    let (_session, mut state, book, graph, _) = fixture();
    let before = state.tree().clone();
    assert_eq!(state.object_users(&[book]), vec!["Graph1"]);
    assert!(
        state.object_users(&[book, graph]).is_empty(),
        "un graphe supprimé avec ses données n'est pas nommé"
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
fn graphe_collable_dans_son_projet_et_embarque_ses_donnees_dans_un_autre() {
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
    let books = stokesforge_app::state::graph_data_books(other.tree(), copied);
    assert_eq!(books.len(), 1);
    let book = books[0];
    assert!(
        matches!(other.tree().node(book).unwrap().kind(), NodeKind::Workbook { meta, .. } if meta.hidden)
    );
    let entry = stokesforge_app::state::ProjectTreeNode::Window {
        id: book,
        name: other.tree().node(book).unwrap().name().into(),
        kind: stokesforge_app::state::WindowKind::Book,
    };
    assert!(!other.project_tree().contains(&entry));
    // #152 : « Show Data » d'une courbe ne révèle que le classeur de cette courbe.
    let series = stokesforge_plot::SeriesRef::new(0, 0);
    assert_eq!(
        stokesforge_app::state::series_data_books(other.tree(), copied, series),
        books
    );
    assert!(
        stokesforge_app::state::series_data_books(
            other.tree(),
            copied,
            stokesforge_plot::SeriesRef::new(0, 99)
        )
        .is_empty(),
        "une courbe absente n'ouvre aucun classeur"
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
        "Show Data du graphique reste disponible"
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
    let mut state = AppState::new_with_preferences(
        session.handle(),
        String::new(),
        stokesforge_app::preferences::Preferences::load_from(None),
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
fn deux_collages_de_graphe_sont_independants_et_figes() {
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
        stokesforge_app::state::graph_data_books(target.tree(), graphs[0]),
        stokesforge_app::state::graph_data_books(target.tree(), graphs[1])
    );
    target.dispatch(Action::Undo);
    target.dispatch(Action::Undo);
    assert_eq!(target.tree(), &before);
}

#[test]
fn les_colonnes_calculees_embarquees_conservent_leurs_valeurs_sans_dependances() {
    let (_session, mut source, _, graph, y) = fixture();
    source.dispatch(Action::SetColumnFormula {
        column: y,
        formula: Some("A*2".into()),
    });
    let mut spec = source.plot_spec(graph).unwrap();
    spec.panels.iter_mut().next().unwrap().series[0].x = None; // A participe au calcul, mais n’est pas tracée.
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

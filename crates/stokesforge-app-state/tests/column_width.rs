//! Largeur des colonnes (#325) côté état de l'application : un réglage de vue
//! qui passe par l'acteur, sans commande ni historique, y compris depuis une
//! vue détachée (son propre `AppState`, la même session).

use stokesforge_app_state::state::{Action, AppState};
use stokesforge_core::{Column, NodeId, Project};
use stokesforge_session::Session;

fn setup(dir: &std::path::Path) -> (Session, AppState, NodeId) {
    let mut project = Project::new("P").unwrap();
    let book = project.add_workbook(project.root(), Some("Book")).unwrap();
    let sheet = project.tree().children(book).unwrap()[0];
    let column = project
        .add_column(sheet, None, Some("A"), Column::from_f64(&[1.0, 2.0]))
        .unwrap();
    let session = Session::spawn(project, None).unwrap();
    let state = AppState::new_with_preferences(
        session.handle(),
        dir.join("p.stf").display().to_string(),
        stokesforge_app_state::preferences::Preferences::load_from(None),
    )
    .unwrap();
    (session, state, column)
}

/// L'action pose la largeur auprès de l'acteur et la relit sans attendre un
/// rafraîchissement ; ni révision ni historique ne bougent.
#[test]
fn l_action_pose_la_largeur_sans_historique() {
    let dir = tempfile::tempdir().unwrap();
    let (session, mut state, column) = setup(dir.path());
    let revision = state.revision();
    assert_eq!(state.column_width(column), None);

    state.dispatch(Action::SetColumnWidth {
        column,
        width: 88.0,
    });

    assert_eq!(state.column_width(column), Some(88.0));
    assert_eq!(state.revision(), revision);
    assert!(!state.can_undo(), "pas d'entrée d'historique");
    let acteur = session.handle().snapshot().unwrap();
    assert_eq!(acteur.column_widths.get(&column), Some(&88.0));
    assert_eq!(acteur.revision, revision);
    // Une largeur invalide n'entre ni dans l'acteur ni dans la copie locale.
    state.dispatch(Action::SetColumnWidth {
        column,
        width: f32::NAN,
    });
    assert_eq!(state.column_width(column), Some(88.0));
}

/// La vue détachée a son propre `AppState` : sa largeur passe aussi par
/// l'acteur, et l'état principal la relit à son prochain rafraîchissement.
#[test]
fn la_vue_detachee_envoie_sa_largeur_a_l_acteur() {
    let dir = tempfile::tempdir().unwrap();
    let (session, mut main, column) = setup(dir.path());
    let mut detached = main.fork_for_detached_view().unwrap();

    detached.dispatch(Action::SetColumnWidth {
        column,
        width: 120.0,
    });

    assert_eq!(detached.column_width(column), Some(120.0));
    assert_eq!(
        session
            .handle()
            .snapshot()
            .unwrap()
            .column_widths
            .get(&column),
        Some(&120.0)
    );
    assert_eq!(main.column_width(column), None, "pas encore rafraîchi");
    main.refresh();
    assert_eq!(main.column_width(column), Some(120.0));
}

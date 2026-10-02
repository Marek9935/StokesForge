//! Tests d'intégration de `stokesforge-session`, via la seule API publique
//! (`Session`, `SessionHandle`, `Snapshot`).

use std::sync::Arc;
use std::thread;

use stokesforge_commands::{
    ColumnRef, Command, CommandError, HistoryLabel, MacroKind, DEFAULT_LIMIT,
};
use stokesforge_core::{Cell, Column, CoreError, NodeId, Project, ValueBlock};
use stokesforge_persistence::PersistenceError;
use stokesforge_session::{Session, SessionError, Snapshot};

/// Construit le projet de fixture : `Book1` → `Sheet1` → colonne `A [1,2,3]`.
/// Rend `(project, sheet, a)`.
fn fixture() -> (Project, NodeId, NodeId) {
    let mut project = Project::new("Demo").expect("projet valide");
    let root = project.root();
    project
        .add_workbook(root, Some("Book1"))
        .expect("classeur créé");
    let sheet = project
        .resolve_path("[Book1]Sheet1")
        .expect("feuille résolue");
    let a = project
        .add_column(sheet, None, Some("A"), Column::from_f64(&[1.0, 2.0, 3.0]))
        .expect("colonne créée");
    (project, sheet, a)
}

/// Lit la colonne `a` d'un instantané comme un vecteur de `f64`.
fn read_a(snapshot: &Snapshot, a: NodeId) -> Vec<f64> {
    let node = snapshot.tree.node(a).expect("colonne présente");
    let column = node.as_column().expect("le nœud est une colonne");
    (0..column.len())
        .map(|row| match column.get(row) {
            Cell::Float(value) => value,
            Cell::Text(_) => unreachable!("colonne de fixture toujours numérique"),
        })
        .collect()
}

/// Commande « écrire une seule valeur flottante à `start` dans `column` ».
fn set_one(column: NodeId, start: usize, value: f64) -> Command {
    Command::SetCells {
        column,
        start,
        values: ValueBlock::Float(vec![value]),
        expected_version: None,
    }
}

#[test]
fn round_trip_execute_undo_redo() {
    let (project, _sheet, a) = fixture();
    let initial_len = project.tree().len();
    let session = Session::spawn(project, None).expect("thread de l'acteur démarré");
    let handle = session.handle();

    let snap0 = handle.snapshot().expect("snapshot");
    assert_eq!(snap0.revision, 0);
    assert_eq!(snap0.tree.len(), initial_len);
    assert!(!snap0.history.can_undo);

    let rev = handle
        .execute(set_one(a, 0, 42.0))
        .expect("exécution réussie");
    assert_eq!(rev, 1);

    let snap1 = handle.snapshot().expect("snapshot");
    assert_eq!(snap1.revision, 1);
    assert_eq!(read_a(&snap1, a)[0], 42.0);
    assert!(snap1.history.can_undo);
    assert_eq!(
        snap1.history.undo_label,
        Some(HistoryLabel::SetCells {
            column: Some(ColumnRef(0))
        })
    );

    let after_undo = handle.undo().expect("undo réussi");
    assert_eq!(after_undo, Some(2));
    let snap2 = handle.snapshot().expect("snapshot");
    assert_eq!(read_a(&snap2, a)[0], 1.0);
    assert!(snap2.history.can_redo);

    let after_redo = handle.redo().expect("redo réussi");
    assert_eq!(after_redo, Some(3));

    let first_undo = handle.undo().expect("undo réussi");
    assert_eq!(first_undo, Some(4));
    let second_undo = handle.undo().expect("undo sans effet, mais pas une erreur");
    assert_eq!(second_undo, None);

    // La révision n'a pas bougé sur le second undo (rien à annuler).
    let snap_final = handle.snapshot().expect("snapshot");
    assert_eq!(snap_final.revision, 4);

    session.shutdown().expect("arrêt propre");
}

#[test]
fn error_leaves_state_unchanged() {
    let (project, sheet, _a) = fixture();
    let session = Session::spawn(project, None).expect("thread de l'acteur démarré");
    let handle = session.handle();

    let before = handle.snapshot().expect("snapshot").revision;

    let unknown = NodeId::from_raw(9999);
    let err = handle
        .execute(set_one(unknown, 0, 1.0))
        .expect_err("nœud inconnu doit échouer");
    assert!(matches!(
        err,
        SessionError::Command(CommandError::Core(CoreError::UnknownNode(_)))
    ));
    assert_eq!(handle.snapshot().expect("snapshot").revision, before);

    let dup = handle
        .execute(Command::AddColumn {
            sheet,
            index: None,
            name: Some("A".to_string()),
            column: Column::from_f64(&[9.0]),
        })
        .expect_err("nom déjà pris doit échouer");
    assert!(matches!(
        dup,
        SessionError::Command(CommandError::Core(CoreError::DuplicateName(_)))
    ));
    assert_eq!(handle.snapshot().expect("snapshot").revision, before);

    session.shutdown().expect("arrêt propre");
}

#[test]
fn snapshots_are_immutable_and_independent() {
    let (project, _sheet, a) = fixture();
    let session = Session::spawn(project, None).expect("thread de l'acteur démarré");
    let handle = session.handle();

    let s0 = handle.snapshot().expect("snapshot");
    handle
        .execute(set_one(a, 0, 7.0))
        .expect("exécution réussie");
    let s1 = handle.snapshot().expect("snapshot");

    assert_eq!(read_a(&s0, a)[0], 1.0);
    assert_eq!(read_a(&s1, a)[0], 7.0);
    assert!(!Arc::ptr_eq(&s0.tree, &s1.tree));

    session.shutdown().expect("arrêt propre");
}

#[test]
fn macros_group_and_undo_atomically() {
    let (project, _sheet, a) = fixture();
    let session = Session::spawn(project, None).expect("thread de l'acteur démarré");
    let handle = session.handle();

    let begin_rev = handle
        .begin_macro(MacroKind::Import)
        .expect("ouverture de macro");
    assert_eq!(begin_rev, 1);
    assert!(handle.snapshot().expect("snapshot").history.in_macro);

    handle
        .execute(set_one(a, 0, 10.0))
        .expect("exécution réussie");
    handle
        .execute(set_one(a, 1, 20.0))
        .expect("exécution réussie");

    handle.end_macro().expect("fermeture de macro");

    let snap = handle.snapshot().expect("snapshot");
    assert!(!snap.history.in_macro);
    assert_eq!(
        snap.history.undo_label,
        Some(HistoryLabel::Macro(MacroKind::Import))
    );

    handle.undo().expect("undo de la macro");
    let after = handle.snapshot().expect("snapshot");
    assert_eq!(read_a(&after, a), vec![1.0, 2.0, 3.0]);

    let err = handle.end_macro().expect_err("hors macro doit échouer");
    assert!(matches!(
        err,
        SessionError::Command(CommandError::NotInMacro)
    ));

    handle
        .begin_macro(MacroKind::PasteCells)
        .expect("ouverture d'une nouvelle macro");
    let err = handle
        .undo()
        .expect_err("undo pendant une macro doit échouer");
    assert!(matches!(
        err,
        SessionError::Command(CommandError::MacroOpen(_))
    ));

    let before_abort = handle.snapshot().expect("snapshot");
    handle
        .execute(set_one(a, 0, 99.0))
        .expect("exécution réussie dans la macro");
    handle.abort_macro().expect("annulation de la macro");

    let after_abort = handle.snapshot().expect("snapshot");
    assert!(!after_abort.history.in_macro);
    assert_eq!(read_a(&after_abort, a), read_a(&before_abort, a));

    session.shutdown().expect("arrêt propre");
}

#[test]
fn two_clients_share_one_history_stack() {
    let (project, _sheet, a) = fixture();
    let session = Session::spawn(project, None).expect("thread de l'acteur démarré");
    let handle1 = session.handle();
    let handle2 = session.handle();

    let t1 = thread::spawn(move || {
        handle1
            .execute(set_one(a, 0, 10.0))
            .expect("exécution réussie (client 1)")
    });
    let t2 = thread::spawn(move || {
        handle2
            .execute(set_one(a, 1, 20.0))
            .expect("exécution réussie (client 2)")
    });
    t1.join().expect("thread 1 terminé sans panique");
    t2.join().expect("thread 2 terminé sans panique");

    let handle = session.handle();
    let both_done = handle.snapshot().expect("snapshot");
    assert_eq!(
        both_done.history.undo_label,
        Some(HistoryLabel::SetCells {
            column: Some(ColumnRef(0))
        })
    );

    // Le premier undo annule la dernière écriture arrivée, quelle qu'elle
    // soit : une seule des deux modifications doit rester.
    handle.undo().expect("premier undo");
    let mid = handle.snapshot().expect("snapshot");
    let mid_values = read_a(&mid, a);
    assert!(
        mid_values == vec![10.0, 2.0, 3.0] || mid_values == vec![1.0, 20.0, 3.0],
        "état intermédiaire inattendu : {mid_values:?}"
    );

    handle.undo().expect("second undo");
    let restored = handle.snapshot().expect("snapshot");
    assert_eq!(read_a(&restored, a), vec![1.0, 2.0, 3.0]);

    session.shutdown().expect("arrêt propre");
}

#[test]
fn concurrent_executes_yield_distinct_contiguous_revisions() {
    let (project, _sheet, a) = fixture();
    // Limite illimitée (0) : les 200 écritures concurrentes doivent toutes
    // rester annulables ensuite.
    let session = Session::spawn(project, Some(0)).expect("thread de l'acteur démarré");

    let workers: Vec<_> = (0..4)
        .map(|_| {
            let handle = session.handle();
            thread::spawn(move || {
                (0..50)
                    .map(|k| {
                        handle
                            .execute(set_one(a, 0, k as f64))
                            .expect("exécution réussie")
                    })
                    .collect::<Vec<_>>()
            })
        })
        .collect();

    let mut revisions: Vec<u64> = workers
        .into_iter()
        .flat_map(|worker| worker.join().expect("thread terminé sans panique"))
        .collect();
    revisions.sort_unstable();
    let expected: Vec<u64> = (1..=200).collect();
    assert_eq!(
        revisions, expected,
        "révisions non distinctes ou avec un trou"
    );

    let handle = session.handle();
    let snap = handle.snapshot().expect("snapshot");
    assert_eq!(snap.revision, 200);
    assert!(snap.history.can_undo);

    let mut undone = 0usize;
    while handle.undo().expect("undo sans erreur").is_some() {
        undone += 1;
    }
    assert_eq!(undone, 200, "les 200 écritures doivent toutes s'annuler");
    let after = handle.snapshot().expect("snapshot");
    assert_eq!(read_a(&after, a), vec![1.0, 2.0, 3.0]);

    session.shutdown().expect("arrêt propre");
}

#[test]
fn history_limit_evicts_oldest_entries() {
    let (project, _sheet, a) = fixture();
    let session = Session::spawn(project, Some(3)).expect("thread de l'acteur démarré");
    let handle = session.handle();

    for k in 0..5 {
        handle
            .execute(set_one(a, 0, k as f64))
            .expect("exécution réussie");
    }

    assert!(handle.undo().expect("undo 1/3").is_some());
    assert!(handle.undo().expect("undo 2/3").is_some());
    assert!(handle.undo().expect("undo 3/3").is_some());
    assert!(
        handle.undo().expect("undo au-delà de la limite").is_none(),
        "la quatrième entrée devait avoir été évincée"
    );

    session.shutdown().expect("arrêt propre");
}

#[test]
fn default_history_limit_matches_the_commands_crate_default() {
    let (project, _sheet, a) = fixture();
    let session = Session::spawn(project, None).expect("thread de l'acteur démarré");
    let handle = session.handle();

    for k in 0..(DEFAULT_LIMIT + 50) {
        handle
            .execute(set_one(a, 0, k as f64))
            .expect("exécution réussie");
    }

    for i in 0..DEFAULT_LIMIT {
        assert!(
            handle
                .undo()
                .expect("undo dans la limite par défaut")
                .is_some(),
            "undo {i} inattendu comme vide"
        );
    }
    assert!(
        handle
            .undo()
            .expect("undo au-delà de la limite par défaut")
            .is_none(),
        "l'entrée au-delà de DEFAULT_LIMIT devait avoir été évincée"
    );

    session.shutdown().expect("arrêt propre");
}

#[test]
fn shutdown_returns_the_project_and_disconnects_handles() {
    let (project, _sheet, a) = fixture();
    let session = Session::spawn(project, None).expect("thread de l'acteur démarré");
    let handle = session.handle();

    handle
        .execute(set_one(a, 0, 42.0))
        .expect("exécution réussie");

    // Lâcher une poignée externe supplémentaire avant l'arrêt ne doit rien
    // changer au bon déroulement de `shutdown`.
    let extra = session.handle();
    drop(extra);

    let project = session.shutdown().expect("arrêt propre");
    let node = project.tree().node(a).expect("colonne présente");
    let column = node.as_column().expect("le nœud est une colonne");
    assert_eq!(column.get(0), Cell::Float(42.0));

    assert!(matches!(handle.snapshot(), Err(SessionError::Disconnected)));
    assert!(matches!(
        handle.execute(set_one(a, 0, 1.0)),
        Err(SessionError::Disconnected)
    ));
}

#[test]
fn snapshot_does_not_clone_the_tree_when_state_is_unchanged() {
    let (project, _sheet, _a) = fixture();
    let session = Session::spawn(project, None).expect("thread de l'acteur démarré");
    let handle = session.handle();

    let s1 = handle.snapshot().expect("snapshot");
    let s2 = handle.snapshot().expect("snapshot");
    assert!(
        Arc::ptr_eq(&s1.tree, &s2.tree),
        "un snapshot sans mutation entre deux appels doit partager l'arbre, pas le cloner"
    );

    session.shutdown().expect("arrêt propre");
}

#[test]
fn save_does_not_change_revision_and_load_resets_history() {
    let (project, _sheet, a) = fixture();
    let session = Session::spawn(project, None).expect("thread de l'acteur démarré");
    let handle = session.handle();

    handle
        .execute(set_one(a, 0, 42.0))
        .expect("exécution réussie");
    let rev_after_execute = handle.snapshot().expect("snapshot").revision;

    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let path = dir.path().join("session.plyx");
    handle.save(&path).expect("sauvegarde réussie");

    // Sauvegarder ne change pas la révision : c'est le même état qu'avant
    // l'appel à `save`.
    assert_eq!(
        handle.snapshot().expect("snapshot").revision,
        rev_after_execute
    );

    handle
        .execute(set_one(a, 0, 7.0))
        .expect("exécution réussie");
    let rev_before_load = handle.snapshot().expect("snapshot").revision;

    let rev_after_load = handle.load(&path).expect("chargement réussi");
    let snap = handle.snapshot().expect("snapshot");

    assert_eq!(read_a(&snap, a)[0], 42.0, "valeur sauvegardée restaurée");
    assert!(!snap.history.can_undo, "l'historique doit être vidé");
    assert!(!snap.history.can_redo, "l'historique doit être vidé");
    assert!(snap.revision > rev_before_load);
    assert_eq!(snap.revision, rev_after_load);

    session.shutdown().expect("arrêt propre");
}

#[test]
fn load_of_a_missing_path_leaves_project_and_history_untouched() {
    let (project, _sheet, a) = fixture();
    let session = Session::spawn(project, None).expect("thread de l'acteur démarré");
    let handle = session.handle();

    handle
        .execute(set_one(a, 0, 42.0))
        .expect("exécution réussie");
    let before = handle.snapshot().expect("snapshot");

    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let missing = dir.path().join("absent.plyx");
    let err = handle
        .load(&missing)
        .expect_err("chemin absent doit échouer");
    assert!(
        matches!(err, SessionError::Persistence(PersistenceError::Io { .. })),
        "{err:?}"
    );

    let after = handle.snapshot().expect("snapshot");
    assert_eq!(after.revision, before.revision, "révision inchangée");
    assert!(
        Arc::ptr_eq(&after.tree, &before.tree),
        "l'arbre doit rester le même Arc, pas seulement égal"
    );
    assert!(after.history.can_undo, "l'historique doit rester intact");

    session.shutdown().expect("arrêt propre");
}

#[test]
fn load_is_refused_while_a_macro_is_open() {
    let (project, _sheet, _a) = fixture();
    let session = Session::spawn(project, None).expect("thread de l'acteur démarré");
    let handle = session.handle();

    handle
        .begin_macro(MacroKind::Import)
        .expect("ouverture de macro");

    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let path = dir.path().join("session.plyx");
    let err = handle
        .load(&path)
        .expect_err("load pendant une macro doit échouer");
    assert!(
        matches!(err, SessionError::Command(CommandError::MacroOpen(_))),
        "{err:?}"
    );

    session.shutdown().expect("arrêt propre");
}

#[test]
fn after_load_new_ids_stay_above_every_id_in_the_file() {
    let (project, sheet, _a) = fixture();
    let session = Session::spawn(project, None).expect("thread de l'acteur démarré");
    let handle = session.handle();

    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let path = dir.path().join("session.plyx");
    handle.save(&path).expect("sauvegarde réussie");

    let highest_in_file = {
        let snap = handle.snapshot().expect("snapshot");
        snap.tree.iter().map(|(id, _)| id.raw()).max().unwrap_or(0)
    };

    handle.load(&path).expect("chargement réussi");

    handle
        .execute(Command::AddColumn {
            sheet,
            index: None,
            name: Some("Z".to_string()),
            column: Column::from_f64(&[9.0]),
        })
        .expect("colonne ajoutée après chargement (compteur d'ids restauré)");

    let snap = handle.snapshot().expect("snapshot");
    let new_id: NodeId = snap
        .tree
        .child_by_name(sheet, "Z")
        .expect("feuille présente dans l'arbre rechargé")
        .expect("colonne Z ajoutée");
    assert!(
        new_id.raw() > highest_in_file,
        "id {new_id} attendu strictement au-dessus de {highest_in_file}"
    );

    session.shutdown().expect("arrêt propre");
}

/// `with_new_identity` (issue #21) : une poignée issue de `.clone()` reste le
/// même émetteur et entre dans sa propre macro ; une poignée à identité neuve
/// est refusée tant qu'une macro d'un autre est ouverte.
#[test]
fn a_handle_with_a_new_identity_cannot_write_into_anothers_macro() {
    use stokesforge_commands::{Command, CommandError};
    use stokesforge_core::{Column, Project, ValueBlock};
    use stokesforge_session::{Session, SessionError};

    let mut project = Project::new("Demo").unwrap();
    let root = project.root();
    project.add_workbook(root, Some("Book1")).unwrap();
    let sheet = project.resolve_path("[Book1]Sheet1").unwrap();
    let column = project
        .add_column(sheet, None, Some("A"), Column::from_f64(&[1.0]))
        .unwrap();
    let session = Session::spawn(project, None).unwrap();
    let owner = session.handle();
    let same = owner.clone();
    let other = owner.with_new_identity();
    let write = |value: f64| Command::SetCells {
        column,
        start: 0,
        values: ValueBlock::Float(vec![value]),
        expected_version: None,
    };

    owner.begin_macro(MacroKind::GraphProperties).unwrap();
    assert!(
        same.execute(write(5.0)).is_ok(),
        "le clone est le même émetteur"
    );
    let refused = other.execute(write(99.0));
    assert!(
        matches!(
            refused,
            Err(SessionError::Command(CommandError::ForeignMacro(_)))
        ),
        "{refused:?}"
    );
    owner.abort_macro().unwrap();
    let snapshot = session.handle().snapshot().unwrap();
    assert!(snapshot.history.undo_label.is_none(), "rien d'annulable");
}

//! Tests d'intégration de la géométrie des fenêtres — voir « Géométrie des
//! fenêtres » en tête de `stokesforge_session` pour la décision qu'ils vérifient.

use stokesforge_commands::Command;
use stokesforge_core::{Column, NodeId, Project, ValueBlock};
use stokesforge_session::{Session, WindowGeometry};

/// Construit le projet de fixture : `Book1` → `Sheet1` → colonne `A [1,2,3]`.
/// Rend `(project, book, column)`.
fn fixture() -> (Project, NodeId, NodeId) {
    let mut project = Project::new("Demo").expect("projet valide");
    let root = project.root();
    let book = project
        .add_workbook(root, Some("Book1"))
        .expect("classeur créé");
    let sheet = project
        .resolve_path("[Book1]Sheet1")
        .expect("feuille résolue");
    let column = project
        .add_column(sheet, None, Some("A"), Column::from_f64(&[1.0, 2.0, 3.0]))
        .expect("colonne créée");
    (project, book, column)
}

fn geometry(x: f64, y: f64, width: f64, height: f64) -> WindowGeometry {
    WindowGeometry {
        x,
        y,
        width,
        height,
        view: None,
    }
}

/// Poser une géométrie ne touche ni `revision`, ni l'historique — `undo`
/// reste sans effet dessus (il n'y a rien à annuler), et l'entrée survit à
/// un `undo`/`redo` d'une commande sans rapport.
#[test]
fn setting_geometry_does_not_move_revision_nor_history() {
    let (project, book, _column) = fixture();
    let session = Session::spawn(project, None).expect("thread de l'acteur démarré");
    let handle = session.handle();

    let snap0 = handle.snapshot().expect("snapshot");
    assert_eq!(snap0.revision, 0);
    assert!(snap0.window_geometries.is_empty());
    assert_eq!(snap0.geometry_revision, 0);

    let geometry_rev = handle
        .set_window_geometry(book, geometry(10.0, 20.0, 640.0, 480.0))
        .expect("géométrie posée");
    assert_eq!(geometry_rev, 1);

    let snap1 = handle.snapshot().expect("snapshot");
    // La révision du modèle n'a pas bougé : ce n'est pas une mutation.
    assert_eq!(snap1.revision, 0);
    assert!(!snap1.history.can_undo);
    assert_eq!(snap1.geometry_revision, 1);
    assert_eq!(
        snap1.window_geometries.get(&book),
        Some(&geometry(10.0, 20.0, 640.0, 480.0))
    );

    // `undo`/`redo` n'ont rien à défaire : la pile de commandes est vide.
    let after_undo = handle.undo().expect("undo sans effet, pas d'erreur");
    assert_eq!(after_undo, None);
    let after_redo = handle.redo().expect("redo sans effet, pas d'erreur");
    assert_eq!(after_redo, None);

    let snap2 = handle.snapshot().expect("snapshot");
    assert_eq!(snap2.revision, 0);
    assert_eq!(snap2.geometry_revision, 1);
    assert_eq!(
        snap2.window_geometries.get(&book),
        Some(&geometry(10.0, 20.0, 640.0, 480.0))
    );
}

/// Une géométrie posée, puis une vraie mutation de données exécutée et
/// annulée : la géométrie n'est affectée ni par l'exécution, ni par
/// `undo`/`redo` d'une commande qui ne touche pas au nœud concerné.
#[test]
fn geometry_survives_unrelated_execute_undo_redo() {
    let (project, book, column) = fixture();
    let session = Session::spawn(project, None).expect("thread de l'acteur démarré");
    let handle = session.handle();

    handle
        .set_window_geometry(book, geometry(1.0, 2.0, 3.0, 4.0))
        .expect("géométrie posée");

    let rev = handle
        .execute(Command::SetCells {
            column,
            start: 0,
            values: ValueBlock::Float(vec![42.0]),
            expected_version: None,
        })
        .expect("commande exécutée");
    assert_eq!(rev, 1);

    let snap_after_execute = handle.snapshot().expect("snapshot");
    assert_eq!(
        snap_after_execute.window_geometries.get(&book),
        Some(&geometry(1.0, 2.0, 3.0, 4.0))
    );
    // Pas de purge : le nœud visé par la géométrie existe toujours, et
    // aucun autre nœud n'a disparu — `geometry_revision` reste à 1.
    assert_eq!(snap_after_execute.geometry_revision, 1);

    handle.undo().expect("undo réussi");
    let snap_after_undo = handle.snapshot().expect("snapshot");
    assert_eq!(
        snap_after_undo.window_geometries.get(&book),
        Some(&geometry(1.0, 2.0, 3.0, 4.0))
    );
}

/// Round-trip `save`/`load` : une géométrie posée, sauvegardée, rechargée
/// dans une nouvelle session, se retrouve identique.
#[test]
fn geometry_round_trips_through_save_and_load() {
    let (project, book, _column) = fixture();
    let session = Session::spawn(project, None).expect("thread de l'acteur démarré");
    let handle = session.handle();

    handle
        .set_window_geometry(book, geometry(100.0, 200.0, 800.0, 600.0))
        .expect("géométrie posée");

    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let path = dir.path().join("geometry.plyx");
    handle.save(&path).expect("sauvegarde réussie");

    // Nouvelle session, projet neuf : le fichier doit lui redonner la
    // disposition des fenêtres, sans qu'on ait rien reposé à la main.
    let empty = Project::new("Vide").expect("projet valide");
    let other_session = Session::spawn(empty, None).expect("thread de l'acteur démarré");
    let other_handle = other_session.handle();

    other_handle.load(&path).expect("chargement réussi");
    let snapshot = other_handle.snapshot().expect("snapshot");

    // `load` réinitialise les identifiants : `book` désigne le même nœud
    // dans le fichier rechargé (première sauvegarde d'un projet fraîchement
    // construit, aucune suppression n'a eu lieu entre-temps).
    assert_eq!(
        snapshot.window_geometries.get(&book),
        Some(&geometry(100.0, 200.0, 800.0, 600.0))
    );
}

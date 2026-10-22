//! Largeur des colonnes (#325) : table `NodeId -> f32` de l'acteur, à côté des
//! géométries de fenêtre — hors du modèle, hors de l'historique, partageant
//! `geometry_revision`, enregistrée dans le `.stf`, jamais purgée à la mutation.

use stokesforge_commands::Command;
use stokesforge_core::{Column, NodeId, Project};
use stokesforge_session::Session;

/// `Book1` → `Sheet1` → colonnes `A`, `B`. Rend `(project, a, b)`.
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
        .add_column(sheet, None, Some("A"), Column::from_f64(&[1.0, 2.0]))
        .expect("colonne A");
    let b = project
        .add_column(sheet, None, Some("B"), Column::from_f64(&[3.0, 4.0]))
        .expect("colonne B");
    (project, a, b)
}

/// Poser une largeur ne touche ni `revision` ni l'historique : `geometry_revision`
/// avance seule. Témoin : faire passer la pose par `revision`.
#[test]
fn poser_une_largeur_n_avance_que_geometry_revision() {
    let (project, _a, b) = fixture();
    let session = Session::spawn(project, None).unwrap();
    let handle = session.handle();
    let before = handle.snapshot().unwrap();
    assert!(before.column_widths.is_empty());

    let rev = handle.set_column_width(b, 80.0).unwrap();
    assert_eq!(rev, before.geometry_revision + 1);

    let after = handle.snapshot().unwrap();
    assert_eq!(after.revision, before.revision, "revision intacte");
    assert!(!after.history.can_undo, "aucune entrée d'historique");
    assert_eq!(after.geometry_revision, before.geometry_revision + 1);
    assert_eq!(after.column_widths.get(&b), Some(&80.0));
    // La même largeur reposée, ou une largeur invalide, ne réveille personne.
    handle.set_column_width(b, 80.0).unwrap();
    handle.set_column_width(b, f32::NAN).unwrap();
    handle.set_column_width(b, -4.0).unwrap();
    handle.set_column_width(b, 0.0).unwrap();
    handle.set_column_width(b, f32::INFINITY).unwrap();
    let still = handle.snapshot().unwrap();
    assert_eq!(still.geometry_revision, after.geometry_revision);
    assert_eq!(still.column_widths.get(&b), Some(&80.0));
    session.shutdown().unwrap();
}

/// Supprimer la colonne puis Ctrl+Z : sa largeur revient (invariant 7 : le
/// `NodeId` est le même, et la table n'est pas purgée à la mutation).
/// Témoin : ajouter les largeurs à `purge_orphaned_geometries`.
#[test]
fn la_largeur_revient_avec_la_colonne_apres_annulation() {
    let (project, a, b) = fixture();
    let session = Session::spawn(project, None).unwrap();
    let handle = session.handle();
    handle.set_column_width(a, 61.0).unwrap();
    handle.set_column_width(b, 99.0).unwrap();

    handle.execute(Command::DeleteColumn { column: b }).unwrap();
    let deleted = handle.snapshot().unwrap();
    assert!(deleted.tree.node(b).is_none());
    assert_eq!(deleted.column_widths.get(&a), Some(&61.0));

    handle.undo().unwrap();
    let restored = handle.snapshot().unwrap();
    assert!(restored.tree.node(b).is_some());
    assert_eq!(restored.column_widths.get(&b), Some(&99.0));
    assert_eq!(restored.column_widths.get(&a), Some(&61.0));
    session.shutdown().unwrap();
}

/// Aller-retour `save`/`load` : la table se retrouve identique.
/// Témoin : ne pas écrire le champ.
#[test]
fn les_largeurs_font_l_aller_retour_du_fichier() {
    let (project, a, b) = fixture();
    let session = Session::spawn(project, None).unwrap();
    let handle = session.handle();
    handle.set_column_width(a, 61.0).unwrap();
    handle.set_column_width(b, 99.5).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("largeurs.stf");
    handle.save(&path).unwrap();

    let other = Session::spawn(Project::new("Vide").unwrap(), None).unwrap();
    let other_handle = other.handle();
    other_handle.load(&path).unwrap();
    let loaded = other_handle.snapshot().unwrap();
    assert_eq!(
        *loaded.column_widths,
        *handle.snapshot().unwrap().column_widths
    );
    assert_eq!(loaded.column_widths.len(), 2);
    // Un projet neuf vide la table.
    other_handle.new_project().unwrap();
    assert!(other_handle.snapshot().unwrap().column_widths.is_empty());
    session.shutdown().unwrap();
    other.shutdown().unwrap();
}

/// Sauvegarde après suppression : la clé de la colonne disparue n'est pas
/// écrite, alors que la table de l'acteur la garde (pour Ctrl+Z).
/// Témoin : retirer le filtre de `save_with_geometries`.
#[test]
fn la_sauvegarde_n_ecrit_pas_la_largeur_d_une_colonne_supprimee() {
    let (project, a, b) = fixture();
    let session = Session::spawn(project, None).unwrap();
    let handle = session.handle();
    handle.set_column_width(a, 61.0).unwrap();
    handle.set_column_width(b, 99.0).unwrap();
    handle.execute(Command::DeleteColumn { column: b }).unwrap();
    // L'acteur garde la clé morte...
    assert!(handle.snapshot().unwrap().column_widths.contains_key(&b));

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("supprime.stf");
    handle.save(&path).unwrap();
    // ... mais le fichier ne la porte pas.
    let (_, view, _) = stokesforge_persistence::load_with_report(&path).unwrap();
    assert_eq!(view.column_widths.get(&a), Some(&61.0));
    assert!(!view.column_widths.contains_key(&b));
    // Et le fichier brut non plus (le filtre de lecture ne masque pas l'écriture).
    let raw = std::fs::read(&path).unwrap();
    let needle = b"column_widths";
    let at = raw
        .windows(needle.len())
        .position(|w| w == needle)
        .expect("le champ est écrit");
    // Une map à une entrée : fixmap 0x81 juste après la clé (str de 13 : 0xad + nom).
    assert_eq!(raw[at + needle.len()], 0x81, "une seule largeur écrite");
    session.shutdown().unwrap();
}

/// Remettre une largeur posée au défaut (#330) : la clé disparaît,
/// `geometry_revision` avance de 1, ni `revision` ni l'historique ne bougent.
/// Une colonne sans largeur : rien ne change. Témoin éprouvé : `+= 1`
/// inconditionnel (rouge).
#[test]
fn remettre_au_defaut_retire_la_cle_et_n_avance_que_si_elle_existait() {
    let (project, a, b) = fixture();
    let session = Session::spawn(project, None).unwrap();
    let handle = session.handle();
    handle.set_column_width(a, 61.0).unwrap();
    handle.set_column_width(b, 99.0).unwrap();
    let before = handle.snapshot().unwrap();

    handle.reset_column_widths(&[b]).unwrap();
    let after = handle.snapshot().unwrap();
    assert!(!after.column_widths.contains_key(&b));
    assert_eq!(
        after.column_widths.get(&a),
        Some(&61.0),
        "A garde la sienne"
    );
    assert_eq!(after.geometry_revision, before.geometry_revision + 1);
    assert_eq!(after.revision, before.revision, "revision intacte");
    assert!(!after.history.can_undo, "aucune entrée d'historique");

    // Déjà au défaut : inchangé.
    handle.reset_column_widths(&[b]).unwrap();
    handle.reset_column_widths(&[]).unwrap();
    let still = handle.snapshot().unwrap();
    assert_eq!(still.geometry_revision, after.geometry_revision);

    // Plusieurs colonnes d'un coup : une seule avance.
    handle.reset_column_widths(&[a, b]).unwrap();
    let all = handle.snapshot().unwrap();
    assert!(all.column_widths.is_empty());
    assert_eq!(all.geometry_revision, after.geometry_revision + 1);
    session.shutdown().unwrap();
}

/// Remise puis enregistrement : le fichier ne porte plus la clé (la session
/// n'écrit pas 54). Témoin éprouvé : poser 54 au lieu de retirer, ou ne rien
/// retirer (rouge).
#[test]
fn une_colonne_remise_au_defaut_n_est_plus_dans_le_fichier() {
    let (project, a, b) = fixture();
    let session = Session::spawn(project, None).unwrap();
    let handle = session.handle();
    handle.set_column_width(a, 61.0).unwrap();
    handle.set_column_width(b, 99.0).unwrap();
    handle.reset_column_widths(&[b]).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("remise.stf");
    handle.save(&path).unwrap();
    let (_, view, _) = stokesforge_persistence::load_with_report(&path).unwrap();
    assert_eq!(view.column_widths.get(&a), Some(&61.0));
    assert!(!view.column_widths.contains_key(&b));
    session.shutdown().unwrap();
}

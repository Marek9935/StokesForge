//! Batterie 18 — ce qui reste possible pendant qu'une macro est ouverte.
//!
//! Une macro promet l'atomicité : ses commandes deviennent **une seule**
//! entrée d'historique, et `abort_macro` ne doit « laisser aucune trace ». La
//! question que cette batterie pose est : *aucune trace où ?* L'acteur garde
//! une macro ouverte entre deux requêtes, et pendant ce temps six autres
//! requêtes restent servies. Deux d'entre elles ne devraient pas l'être.
//!
//! `Request::Load` le sait déjà — il rend `Err(MacroOpen)` avant même de
//! toucher au disque. `Request::Save`, trois lignes plus haut dans le même
//! `match`, ne le sait pas.

use plotyx_commands::{Command, History};
use plotyx_core::{Column, NodeId, Project, ValueBlock};
use plotyx_session::{Session, SessionError, SessionHandle};
use tempfile::TempDir;

/// `[Book1]Sheet1!A` = [1, 2, 3], servie par un acteur.
fn session() -> (Session, SessionHandle, NodeId) {
    let mut project = Project::new("Demo").expect("projet valide");
    let root = project.root();
    project.add_workbook(root, Some("Book1")).expect("classeur");
    let sheet = project.resolve_path("[Book1]Sheet1").expect("feuille");
    let colonne = project
        .add_column(sheet, None, Some("A"), Column::from_f64(&[1.0, 2.0, 3.0]))
        .expect("colonne A");
    let session = Session::spawn(project, None).expect("acteur démarré");
    let handle = session.handle();
    (session, handle, colonne)
}

fn valeurs(project: &Project, colonne: NodeId) -> Vec<f64> {
    project
        .tree()
        .node(colonne)
        .and_then(plotyx_core::Node::as_column)
        .and_then(Column::float_values)
        .expect("colonne numérique")
        .as_slice()
        .to_vec()
}

fn ecrire(handle: &SessionHandle, colonne: NodeId, valeur: f64) {
    handle
        .execute(Command::SetCells {
            column: colonne,
            start: 0,
            values: ValueBlock::Float(vec![valeur]),
        })
        .expect("écriture acceptée");
}

// -- garde-fous -----------------------------------------------------------

#[test]
fn b18_le_chargement_est_refuse_pendant_une_macro() {
    let dir = TempDir::new().expect("répertoire");
    let chemin = dir.path().join("autre.plx");
    let (_session, handle, colonne) = session();
    handle.save(&chemin).expect("sauvegarde initiale");
    handle.begin_macro("import").expect("macro ouverte");
    ecrire(&handle, colonne, 99.0);
    assert!(
        matches!(
            handle.load(&chemin),
            Err(SessionError::Command(
                plotyx_commands::CommandError::MacroOpen(_)
            ))
        ),
        "`load` doit refuser tant qu'une macro est ouverte — et il le fait"
    );
}

#[test]
fn b18_une_macro_avortee_ne_laisse_rien_dans_le_modele() {
    let (session, handle, colonne) = session();
    handle.begin_macro("import").expect("macro ouverte");
    ecrire(&handle, colonne, 99.0);
    handle.abort_macro().expect("macro avortée");
    let project = session.shutdown().expect("arrêt");
    assert_eq!(valeurs(&project, colonne), vec![1.0, 2.0, 3.0]);
}

#[test]
fn b18_une_macro_de_profondeur_raisonnable_sannule_dun_bloc() {
    let mut project = Project::new("Demo").expect("projet valide");
    let root = project.root();
    project.add_workbook(root, Some("Book1")).expect("classeur");
    let sheet = project.resolve_path("[Book1]Sheet1").expect("feuille");
    let colonne = project
        .add_column(sheet, None, Some("A"), Column::from_f64(&[1.0]))
        .expect("colonne");
    let mut history = History::with_limit(0);
    for i in 0..1000 {
        history.begin_macro(&format!("m{i}"));
    }
    history
        .execute(
            &mut project,
            Command::SetCells {
                column: colonne,
                start: 0,
                values: ValueBlock::Float(vec![9.0]),
            },
        )
        .expect("écriture");
    for _ in 0..1000 {
        history.end_macro(&mut project).expect("fermeture");
    }
    assert_eq!(history.len(), 1, "mille niveaux, une seule entrée");
    assert_eq!(history.undo(&mut project), Ok(true));
    assert_eq!(valeurs(&project, colonne), vec![1.0]);
}

// -- reproductions --------------------------------------------------------

#[test]
fn b18_la_sauvegarde_est_refusee_pendant_une_macro() {
    let dir = TempDir::new().expect("répertoire");
    let chemin = dir.path().join("projet.plx");
    let (session, handle, colonne) = session();

    handle
        .begin_macro("import d'un fichier de mesures")
        .expect("macro");
    ecrire(&handle, colonne, 99.0);
    let sauvegarde = handle.save(&chemin);
    println!("save pendant la macro → {sauvegarde:?}");

    if sauvegarde.is_ok() {
        let sur_disque = plotyx_persistence::load(&chemin).expect("relecture");
        println!("le fichier contient : {:?}", valeurs(&sur_disque, colonne));
        handle.abort_macro().expect("macro avortée");
        let apres = session.shutdown().expect("arrêt");
        println!(
            "le modèle, après l'abandon : {:?}",
            valeurs(&apres, colonne)
        );
    }

    assert!(
        matches!(
            sauvegarde,
            Err(SessionError::Command(
                plotyx_commands::CommandError::MacroOpen(_)
            ))
        ),
        "le fichier écrit contient l'état intermédiaire d'une macro que \
         l'utilisateur abandonne ensuite : le modèle revient à [1, 2, 3], le disque \
         garde [99, 2, 3], et aucune annulation ne peut plus l'atteindre. \
         `Request::Load` refuse déjà ce cas (`Err(MacroOpen)`, garde-fou \
         b18_le_chargement_est_refuse_pendant_une_macro) ; `Request::Save`, dans le \
         même `match` de `actor.rs`, ne consulte pas `history.in_macro()`"
    );
}

#[test]
#[ignore = "reproduction B7 — rien ne borne l'imbrication des macros ; au-delà de 10 890 niveaux, l'annulation déborde la pile du fil de l'acteur et fait abort() le processus"]
fn b18_limbrication_des_macros_est_bornee() {
    // Sûr : **construire** l'imbrication ne déborde jamais (`begin_macro` et
    // `end_macro` sont itératifs ; la sonde le vérifie jusqu'à 200 000). Seule
    // sa traversée récurse — `History::revert` pour `Entry::Macro`, et le
    // `Drop` dérivé de `Entry`. Ce test ne la traverse donc pas : il constate
    // seulement qu'aucune borne n'existe.
    const PROFONDEUR_SURE: usize = 10_000;
    let mut history = History::with_limit(0);
    for i in 0..50_000 {
        history.begin_macro(&format!("m{i}"));
    }
    println!(
        "profondeur atteinte sans le moindre refus : {}",
        history.macro_depth()
    );
    println!(
        "seuils mesurés hors test, pile de 2 Mio (celle du fil de l'acteur) :\n  \
         construire 200 000 niveaux : OK\n  \
         annuler   : passe à 10 890, abort() à 10 891\n  \
         `clear()` : passe à 26 147, abort() à 26 148  ← c'est ce que `load` appelle\n  \
         lâcher    : passe à 26 145, abort() à 26 146\n  \
         (`cargo run --bin plotyx-sonde-pile -- <n> <geste>`)"
    );
    assert!(
        history.macro_depth() <= PROFONDEUR_SURE,
        "l'historique a accepté {} niveaux d'imbrication. Un débordement de pile \
         n'est pas une panique : le runtime appelle `abort()`, donc ni `catch_unwind` \
         ni l'acteur ne peuvent intervenir — le processus meurt avec tout le travail \
         non sauvegardé. Une borne explicite (`Err(CommandError::…)` au-delà d'une \
         profondeur nommée) coûte trois lignes ; rendre `revert` et `Drop` itératifs \
         coûte davantage mais lève la limite",
        history.macro_depth()
    );
}

#[test]
fn b18_linterface_nannonce_pas_une_annulation_impossible() {
    let (_session, handle, colonne) = session();
    ecrire(&handle, colonne, 5.0);
    handle.begin_macro("groupe").expect("macro");

    let instantane = handle.snapshot().expect("instantané");
    println!(
        "pendant la macro : can_undo = {}, undo_label = {:?}, in_macro = {}",
        instantane.history.can_undo, instantane.history.undo_label, instantane.history.in_macro
    );
    println!("undo → {:?}", handle.undo());

    assert!(
        !instantane.history.can_undo || !instantane.history.in_macro,
        "l'instantané annonce « Annuler {:?} » alors que toute annulation rendra \
         `Err(MacroOpen)`. `HistoryState` porte déjà `in_macro` : c'est `can_undo` \
         (et `AppState::can_undo`, qui le recopie tel quel) qui ne le consulte pas — \
         le menu propose une action qui échouera à coup sûr",
        instantane.history.undo_label
    );
}

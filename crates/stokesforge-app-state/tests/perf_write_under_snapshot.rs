//! Mesure du coût d'une saisie de cellule quand l'interface **détient** un
//! instantané entre chaque frappe, comparé au cas où elle ne le fait pas.
//!
//! # Ce test n'a de sens qu'en profil release
//!
//! Il chronomètre une copie mémoire sur un million de `f64` ; en profil
//! debug, les temps sont dominés par l'absence d'optimisation du code
//! généré et ne renseignent rien sur le coût réel de l'architecture. À
//! lancer avec :
//!
//! ```text
//! cargo test -p stokesforge-app-state --release --test perf_write_under_snapshot -- --ignored --nocapture
//! ```
//!
//! Le test est marqué `#[ignore]` : il alloue une colonne de 8 Mo et prend
//! plusieurs centaines de millisecondes, ce qui n'a pas sa place dans
//! `cargo test` ordinaire.
//!
//! # Ce qui est mesuré, et pourquoi
//!
//! Une colonne vit dans le modèle comme un `Arc<Vec<f64>>`, muté par
//! `Arc::make_mut` (`stokesforge_core::column`, `stokesforge_core::tree`). Tant que
//! l'`Arc` n'a qu'un seul propriétaire, `Arc::make_mut` écrit en place ; dès
//! qu'il en a deux, il clone le `Vec` avant d'écrire. Or `SessionHandle::
//! snapshot` (`stokesforge_session::actor::build_snapshot`) rend l'arbre courant
//! par un simple `Arc::clone` — et l'interface **garde en permanence** le
//! dernier instantané reçu, pour peindre sans jamais verrouiller l'acteur.
//! Donc, dans l'usage réel de `AppState`, au moment où une commande
//! `SetCells` atteint l'acteur, l'instantané précédent (celui que l'UI vient
//! de peindre) est encore vivant côté client : la colonne est partagée, et
//! l'écriture doit recopier tout le buffer avant de modifier une seule
//! cellule.
//!
//! Ce coût est borné (une colonne, pas tout le modèle) et ne s'accumule pas
//! en mémoire (l'ancien buffer est libéré dès que le dernier instantané qui
//! le référence est lâché). Mais il doit être mesuré, pas supposé : c'est ce
//! que fait ce test, en comparant deux scénarios qui n'exécutent que le
//! *strict* nécessaire pour compiler le même nombre de commandes et de
//! rafraîchissements — seule la durée de vie de l'instantané intermédiaire
//! change.
//!
//! # Ce que la mesure a montré, et qui déplace la conclusion attendue
//!
//! Les deux scénarios rendent des moyennes **quasi identiques**. Ce n'est
//! pas un bug du test : `History::execute` (`stokesforge-commands/src/
//! history.rs`) clone `project.tree()` (`let before = Arc::clone(project.
//! tree());`) **avant d'appliquer chaque commande**, pour calculer son
//! libellé et pouvoir restaurer l'arbre en cas d'échec. Ce `before` local
//! alias l'arbre le temps de l'application, ce qui déclenche la même cascade
//! de copie-à-l'écriture jusqu'au `Vec` de la colonne — **que l'appelant
//! tienne ou non un instantané**. Vérifié par comparaison d'adresse
//! (`Arc::as_ptr`) : même une suite d'écritures sans qu'aucun client ne
//! demande jamais d'instantané réalloue le buffer à chaque commande.
//!
//! Conséquence pour la documentation d'architecture : la recopie par
//! écriture n'est donc pas un coût que l'UI pourrait éviter en relâchant son
//! instantané plus tôt — elle est déjà payée par `History::execute` pour
//! *tout* client, y compris un futur pont Python qui n'afficherait rien.
//! Réduire ce coût demanderait de changer `History::execute` lui-même (par
//! exemple ne cloner `project.tree()` qu'à la demande, ou ne le faire que
//! pour les commandes de nature « structure », qui en ont réellement besoin
//! pour leur restauration en `Arc<Tree>` — voir la doc de `history.rs`).

use std::time::{Duration, Instant};

use stokesforge_commands::Command;
use stokesforge_core::{Column, NodeId, Project, ValueBlock};
use stokesforge_session::{Session, SessionHandle, Snapshot};

/// Longueur de la colonne mesurée : un million de `f64`, soit 8 Mo — assez
/// gros pour qu'une recopie complète se distingue clairement du bruit de
/// mesure.
const COLUMN_LEN: usize = 1_000_000;
/// Nombre d'écritures moyennées par scénario.
const ITERATIONS: usize = 100;

/// Construit un projet à une seule feuille portant une colonne `Large` de
/// [`COLUMN_LEN`] valeurs, puis démarre sa session. Rend la [`Session`] (à
/// garder vivante : la lâcher termine l'acteur) et l'identifiant de la
/// colonne mesurée.
fn spawn_session_with_large_column() -> (Session, NodeId) {
    let mut project = Project::new("Perf").expect("projet de test");
    let root = project.root();
    let book = project
        .add_workbook(root, Some("Book1"))
        .expect("classeur Book1");
    let sheet = project
        .tree()
        .child_by_name(book, "Sheet1")
        .expect("Book1 lisible")
        .expect("Sheet1 créée avec le classeur");
    let values = vec![0.0_f64; COLUMN_LEN];
    let column = project
        .add_column(sheet, None, Some("Large"), Column::from_f64(&values))
        .expect("colonne Large");
    let session = Session::spawn(project, None).expect("thread de session");
    (session, column)
}

/// Commande qui écrit une unique cellule à la ligne `row` de `column`.
fn set_one_cell(column: NodeId, row: usize) -> Command {
    Command::SetCells {
        column,
        start: row,
        values: ValueBlock::Float(vec![row as f64]),
        expected_version: None,
    }
}

/// Scénario réel de `AppState` : le dernier instantané reçu est **gardé
/// vivant** jusqu'au rafraîchissement suivant. À chaque tour, l'instantané
/// tenu depuis le tour précédent alias encore l'arbre de l'acteur au moment
/// où la commande l'atteint, puis il est remplacé par le nouvel instantané,
/// qui jouera le même rôle au tour suivant.
fn measure_writes_holding_snapshot(handle: &SessionHandle, column: NodeId) -> Duration {
    let mut held: Snapshot = handle.snapshot().expect("instantané initial");
    let mut total = Duration::ZERO;
    for row in 0..ITERATIONS {
        let start = Instant::now();
        handle
            .execute(set_one_cell(column, row))
            .expect("écriture de cellule");
        // Remplace `held` : l'ancien instantané n'est lâché qu'ici, après que
        // la commande a déjà dû composer avec lui.
        held = handle.snapshot().expect("instantané rafraîchi");
        total += start.elapsed();
    }
    drop(held);
    total
}

/// Même suite d'écritures, mais l'instantané est relâché **aussitôt reçu** :
/// aucun client n'alias plus la colonne quand la commande suivante part (voir
/// cependant la note de tête de fichier : cela ne suffit plus à éviter la
/// copie, `History::execute` en aliasant une elle-même le temps de chaque
/// application).
fn measure_writes_releasing_snapshot(handle: &SessionHandle, column: NodeId) -> Duration {
    let mut total = Duration::ZERO;
    for row in 0..ITERATIONS {
        let start = Instant::now();
        handle
            .execute(set_one_cell(column, row))
            .expect("écriture de cellule");
        let snapshot = handle.snapshot().expect("instantané");
        drop(snapshot);
        total += start.elapsed();
    }
    total
}

#[test]
#[ignore = "coûteux (colonne d'un million de f64) et n'a de sens qu'en profil release — voir la doc de ce fichier"]
fn measures_cell_write_cost_with_and_without_a_held_snapshot() {
    let (session_held, column_held) = spawn_session_with_large_column();
    let handle_held = session_held.handle();
    let held_total = measure_writes_holding_snapshot(&handle_held, column_held);

    let (session_released, column_released) = spawn_session_with_large_column();
    let handle_released = session_released.handle();
    let released_total = measure_writes_releasing_snapshot(&handle_released, column_released);

    let held_avg_ms = held_total.as_secs_f64() * 1_000.0 / ITERATIONS as f64;
    let released_avg_ms = released_total.as_secs_f64() * 1_000.0 / ITERATIONS as f64;

    println!();
    println!(
        "Coût moyen d'une écriture de cellule sous {COLUMN_LEN} lignes ({ITERATIONS} écritures) :"
    );
    println!("  instantané détenu entre les écritures : {held_avg_ms:>9.4} ms");
    println!("  instantané relâché aussitôt reçu      : {released_avg_ms:>9.4} ms");

    // Les deux scénarios doivent avoir produit le même état final : sinon la
    // mesure ne compare pas deux fois la même chose.
    let final_held = handle_held.snapshot().expect("instantané final (détenu)");
    let final_released = handle_released
        .snapshot()
        .expect("instantané final (relâché)");
    assert_eq!(
        *final_held.tree, *final_released.tree,
        "les deux scénarios doivent produire le même arbre final"
    );

    assert!(
        session_held.shutdown().is_ok(),
        "arrêt propre de la session (détenu)"
    );
    assert!(
        session_released.shutdown().is_ok(),
        "arrêt propre de la session (relâché)"
    );
}

//! Tests d'intégration du fil de script (tâches 5.3 à 5.5).
//!
//! Ils ne s'exécutent qu'avec la feature `python` et exigent le préfixe
//! vendorisé : lance `scripts/python-vendor.sh` puis `scripts/python-test.sh`.
//!
//! Chaque test correspond à un critère d'acceptation. Un seul mérite d'être lu
//! avant les autres : [`critere_de_fin_lecture_sans_copie_ecriture_par_le_bus_annulation_par_ctrl_z`],
//! qui traverse d'un coup l'acteur, le GIL, la durée de vie des buffers
//! partagés et l'historique. Tant qu'il ne passe pas, le pont n'est pas prouvé.
//!
//! Comme dans `plotyx-python/tests/zerocopy.rs`, CPython ne démarre qu'une fois
//! par processus ; ici c'est le fil de script du **premier** moteur créé qui
//! s'en charge, et les moteurs suivants réutilisent l'interpréteur.

#![cfg(feature = "python")]

use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{mpsc, Arc, OnceLock, PoisonError, RwLock, RwLockReadGuard, RwLockWriteGuard};
use std::thread;
use std::time::{Duration, Instant};

use plotyx_commands::Command;
use plotyx_core::{Column, NodeId, Project, Tree};
use plotyx_python::interpreter::{self, EmbeddedPython};
use plotyx_script::{ScriptEngine, ScriptError, ScriptOutcome};
use plotyx_session::{Session, SessionHandle};
use pyo3::prelude::*;

/// Résout le préfixe embarqué : `PLOTYX_PYTHON_HOME` si posée par le script de
/// test, sinon `vendor/python/extracted/python` relatif au manifeste.
fn home() -> PathBuf {
    match std::env::var_os(interpreter::HOME_ENV) {
        Some(value) => PathBuf::from(value),
        None => {
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../vendor/python/extracted/python")
        }
    }
}

/// Verrou d'accès à l'interpréteur, **partagé par tout le binaire de tests**.
///
/// `sys.setswitchinterval` est un réglage **global à l'interpréteur**. Le
/// critère 11 doit le pousser très haut pour être décisif — et pendant ce
/// temps, tout autre fil Python du binaire est affamé : mesuré, un seul test
/// laissé en parallèle de celui-là faisait passer la durée de la suite de
/// 0,1 s à plus de 60 s. Ce verrou lecteur/rédacteur dit exactement la
/// contrainte : tout test qui exécute un script prend l'accès **partagé** ;
/// le seul qui déplace l'intervalle de commutation prend l'accès
/// **exclusif**.
fn verrou_interpreteur() -> &'static RwLock<()> {
    static VERROU: OnceLock<RwLock<()>> = OnceLock::new();
    VERROU.get_or_init(|| RwLock::new(()))
}

/// L'accès à l'interpréteur détenu par un banc, gardé vivant tant que le banc
/// l'est.
///
/// Les gardes ne sont jamais *lus* : leur seule raison d'être est leur durée
/// de vie, d'où le `allow` — un garde qu'on lirait serait un garde qu'on
/// aurait mal compris.
#[allow(dead_code)]
enum Acces {
    /// Cohabite avec les autres tests.
    Partage(RwLockReadGuard<'static, ()>),
    /// Exclut tout autre test qui exécuterait du Python.
    Exclusif(RwLockWriteGuard<'static, ()>),
}

/// Le banc : un projet « Demo », son acteur, et un moteur de script branché
/// dessus.
///
/// `Book1` contient `Sheet1` avec `A = [1, 2, 3]`, `B = [10, 20, 30]` et une
/// colonne texte `T` — cette dernière n'est pas dans la fixture du cahier des
/// charges, elle sert au seul critère 6 (« `column` refuse ce qu'il doit »).
struct Banc {
    /// Le moteur, dans un `Arc` pour pouvoir le partager avec un fil de test
    /// qu'on doit pouvoir abandonner si un critère borné expire.
    engine: Arc<ScriptEngine>,
    /// Poignée cliente vers l'acteur, côté Rust (« ce que ferait la fenêtre »).
    session: SessionHandle,
    /// L'acteur lui-même ; le garder en vie est tout son rôle ici.
    _actor: Session,
    /// Colonne `A`.
    a: NodeId,
    /// Colonne `B`.
    b: NodeId,
    /// Accès à l'interpréteur, relâché à la destruction du banc.
    _acces: Acces,
}

/// Monte un banc neuf, en cohabitation avec les autres tests.
fn banc() -> Banc {
    monter(Acces::Partage(
        verrou_interpreteur()
            .read()
            .unwrap_or_else(PoisonError::into_inner),
    ))
}

/// Monte un banc neuf qui **exclut** tout autre test exécutant du Python.
fn banc_exclusif() -> Banc {
    monter(Acces::Exclusif(
        verrou_interpreteur()
            .write()
            .unwrap_or_else(PoisonError::into_inner),
    ))
}

/// Monte le projet « Demo », son acteur et un moteur de script branché dessus.
fn monter(acces: Acces) -> Banc {
    let mut project = Project::new("Demo").expect("projet Demo");
    let root = project.root();
    project
        .add_workbook(root, Some("Book1"))
        .expect("classeur Book1");
    let sheet = project
        .resolve_path("[Book1]Sheet1")
        .expect("feuille Sheet1");
    let a = project
        .add_column(sheet, None, Some("A"), Column::from_f64(&[1.0, 2.0, 3.0]))
        .expect("colonne A");
    let b = project
        .add_column(
            sheet,
            None,
            Some("B"),
            Column::from_f64(&[10.0, 20.0, 30.0]),
        )
        .expect("colonne B");
    project
        .add_column(
            sheet,
            None,
            Some("T"),
            Column::from_text(&[Some("x".to_owned()), None, Some("z".to_owned())]),
        )
        .expect("colonne T");

    let actor = Session::spawn(project, None).expect("acteur démarré");
    let session = actor.handle();
    let engine = ScriptEngine::spawn(session.clone(), EmbeddedPython::with_home(home()))
        .expect("moteur de script démarré");

    Banc {
        engine: Arc::new(engine),
        session,
        _actor: actor,
        a,
        b,
        _acces: acces,
    }
}

/// Exécute un script et exige qu'il se termine sans erreur ; rend la sortie.
fn executer(banc: &Banc, source: &str) -> ScriptOutcome {
    let outcome = banc
        .engine
        .run(source)
        .expect("le moteur accepte le script");
    assert!(
        outcome.error.is_none(),
        "script en échec :\n{}",
        outcome.error.clone().unwrap_or_default()
    );
    outcome
}

/// L'import Python emploie le modèle fourni et la conversion commune ; un
/// seul Ctrl+Z retire classeur, feuilles, colonnes et métadonnées.
#[test]
fn import_python_par_modele_se_lit_et_sannule_en_un_geste() {
    let banc = banc();
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../test/import/jasco_uvvis_semicolon.csv");
    let before = banc.session.snapshot().unwrap();
    executer(
        &banc,
        &format!(
            "feuilles = pyx.import_file({:?}, recipe='Jasco (point-virgule)')\n\
         assert len(feuilles) == 1\n",
            path.to_str().unwrap()
        ),
    );
    let imported = banc.session.snapshot().unwrap();
    let root = imported.tree.root();
    let book = *imported.tree.node(root).unwrap().children().last().unwrap();
    let sheets = imported.tree.node(book).unwrap().children();
    assert_eq!(sheets.len(), 1, "aucune feuille vide supplémentaire");
    let columns = imported.tree.node(sheets[0]).unwrap().children();
    let x = imported.tree.node(columns[0]).unwrap().as_column().unwrap();
    let y = imported.tree.node(columns[1]).unwrap().as_column().unwrap();
    assert_eq!(x.meta().role, plotyx_core::ColumnRole::X);
    assert_eq!(
        x.float_values().unwrap().as_slice(),
        &[800., 798., 796., 794., 792., 790.]
    );
    assert_eq!(y.meta().long_name, "Fixture_Jasco_UVVis");
    assert!(y.meta().file_metadata.iter().any(|m| m.key == "XUNITS"));
    assert!(y.meta().import_source.is_some());
    assert_eq!(imported.history.undo_label.as_deref(), Some("Importer"));
    executer(
        &banc,
        &format!(
            "x = pyx.resolve('[{}]{}!{}')\n\
         assert list(pyx.column(x)) == [800., 798., 796., 794., 792., 790.]\n\
         assert pyx.undo()\n",
            imported.tree.node(book).unwrap().name(),
            imported.tree.node(sheets[0]).unwrap().name(),
            imported.tree.node(columns[0]).unwrap().name()
        ),
    );
    let undone = banc.session.snapshot().unwrap();
    assert_eq!(*undone.tree, *before.tree);
    assert!(!undone.history.can_undo);
    banc.session.redo().unwrap();
    assert_eq!(*banc.session.snapshot().unwrap().tree, *imported.tree);
}

/// Le choix automatique utilise la même signature Jasco que l'application.
#[test]
fn import_python_choisit_le_modele_automatiquement() {
    let banc = banc();
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../test/import/jasco_uvvis_semicolon.csv");
    executer(
        &banc,
        &format!(
            "from pathlib import Path\nassert len(pyx.import_file(Path({:?}))) == 1",
            path.to_str().unwrap()
        ),
    );
    let snapshot = banc.session.snapshot().unwrap();
    let source = snapshot
        .tree
        .walk_all()
        .into_iter()
        .filter_map(|id| {
            snapshot
                .tree
                .node(id)?
                .as_column()?
                .meta()
                .import_source
                .as_ref()
        })
        .next()
        .unwrap();
    assert!(source.recipe_json.contains("Jasco (point-virgule)"));
}

/// Modèle inconnu, fichier absent ou binaire : aucune mutation et aucune
/// macro laissée ouverte. Une macro de l'interface n'est jamais absorbée.
#[test]
fn import_python_refuse_sans_toucher_au_projet_ni_aux_macros_dautrui() {
    let banc = banc();
    let dir = tempfile::tempdir().unwrap();
    let binary = dir.path().join("binaire.dat");
    std::fs::write(&binary, [0u8, 159, 146, 150, 0, 0]).unwrap();
    let absent = dir.path().join("absent.csv");
    let before = banc.session.snapshot().unwrap();
    for source in [
        format!(
            "pyx.import_file({:?}, recipe='Modèle inexistant')",
            binary.to_str().unwrap()
        ),
        format!("pyx.import_file({:?})", absent.to_str().unwrap()),
        format!("pyx.import_file({:?})", binary.to_str().unwrap()),
    ] {
        let outcome = banc.engine.run(&source).unwrap();
        assert!(
            outcome
                .error
                .as_deref()
                .is_some_and(|e| e.contains("InvalidValueError")),
            "{outcome:?}"
        );
        let snapshot = banc.session.snapshot().unwrap();
        assert_eq!(*snapshot.tree, *before.tree);
        assert_eq!(snapshot.revision, before.revision);
        assert!(!snapshot.history.in_macro);
    }
    let csv = dir.path().join("mesure.csv");
    std::fs::write(&csv, "1,10\n2,20\n").unwrap();
    let interface = banc._actor.handle();
    interface.begin_macro("Édition de l'interface").unwrap();
    let outcome = banc
        .engine
        .run(&format!("pyx.import_file({:?})", csv.to_str().unwrap()))
        .unwrap();
    assert!(outcome
        .error
        .as_deref()
        .is_some_and(|e| e.contains("InvalidValueError")));
    let snapshot = banc.session.snapshot().unwrap();
    assert_eq!(*snapshot.tree, *before.tree);
    assert!(
        snapshot.history.in_macro,
        "la macro de l'interface reste ouverte"
    );
    interface.abort_macro().unwrap();
}

/// Valeurs actuelles d'une colonne numérique, lues côté Rust.
fn valeurs(session: &SessionHandle, id: NodeId) -> Vec<f64> {
    session
        .snapshot()
        .expect("instantané")
        .tree
        .node(id)
        .expect("nœud présent")
        .as_column()
        .expect("colonne")
        .float_values()
        .expect("colonne numérique")
        .as_slice()
        .to_vec()
}

/// Adresse du `Vec<f64>` d'une colonne, telle que Rust la voit.
fn adresse_rust(session: &SessionHandle, id: NodeId) -> usize {
    session
        .snapshot()
        .expect("instantané")
        .tree
        .node(id)
        .expect("nœud présent")
        .as_column()
        .expect("colonne")
        .float_values()
        .expect("colonne numérique")
        .as_ptr() as usize
}

/// Arbre courant du projet.
fn arbre(session: &SessionHandle) -> Arc<Tree> {
    session.snapshot().expect("instantané").tree
}

/// Révision courante du projet.
fn revision(session: &SessionHandle) -> u64 {
    session.snapshot().expect("instantané").revision
}

/// Les lignes de la sortie standard d'un script, sans les blancs de bordure.
fn lignes(outcome: &ScriptOutcome) -> Vec<String> {
    outcome
        .stdout
        .lines()
        .map(|line| line.trim().to_owned())
        .collect()
}

/// Attend que `condition` devienne vraie, au plus `delai`. Rend `false` si le
/// délai expire — aucun test ne doit attendre indéfiniment.
fn attendre(delai: Duration, mut condition: impl FnMut() -> bool) -> bool {
    let limite = Instant::now() + delai;
    while Instant::now() < limite {
        if condition() {
            return true;
        }
        thread::sleep(Duration::from_millis(1));
    }
    condition()
}

// --- 1. Le moteur démarre ---------------------------------------------------

#[test]
fn le_moteur_demarre_et_execute_un_script_vide() {
    let banc = banc();
    assert!(!banc.engine.is_running(), "aucun script au démarrage");

    let outcome = executer(&banc, "");
    assert_eq!(outcome, ScriptOutcome::default());
    assert!(outcome.succeeded());
    assert!(!banc.engine.is_running(), "plus rien ne tourne après coup");

    Arc::try_unwrap(banc.engine)
        .expect("un seul détenteur du moteur")
        .shutdown()
        .expect("arrêt propre");
}

// --- 2. `print` capturé -----------------------------------------------------

/// Marqueur assez singulier pour qu'aucune autre sortie du binaire de tests ne
/// puisse le produire par hasard.
const MARQUEUR: &str = "bonjour-4f1c9a2e-temoin";

/// Détourne le descripteur 1 du **processus** vers un tuyau le temps de
/// `action`, puis rend ce qui y a coulé.
///
/// C'est la seule façon honnête de vérifier « rien n'apparaît sur la sortie du
/// processus » : `sys.stdout` vu de Python ne dit rien de ce qui atteint
/// réellement le descripteur. L'assertion porte sur l'**absence du marqueur**,
/// et non sur un tuyau vide : d'autres tests peuvent écrire sur la sortie du
/// processus pendant cette fenêtre (le harnais `libtest`, notamment).
fn sortie_du_processus_pendant<T>(action: impl FnOnce() -> T) -> (T, String) {
    let (sauvegarde, lecture) = Python::attach(|py| {
        let os = py.import("os").expect("import os");
        let sauvegarde: i32 = os
            .call_method1("dup", (1,))
            .expect("dup")
            .extract()
            .expect("descripteur");
        let tuyau = os.call_method0("pipe").expect("pipe");
        let lecture: i32 = tuyau
            .get_item(0)
            .expect("bout de lecture")
            .extract()
            .expect("descripteur");
        let ecriture: i32 = tuyau
            .get_item(1)
            .expect("bout d'écriture")
            .extract()
            .expect("descripteur");
        os.call_method1("dup2", (ecriture, 1)).expect("dup2");
        os.call_method1("close", (ecriture,)).expect("close");
        (sauvegarde, lecture)
    });

    let valeur = action();

    let fuite = Python::attach(|py| {
        let os = py.import("os").expect("import os");
        // Remettre la vraie sortie en place ferme le dernier bout d'écriture du
        // tuyau : la lecture qui suit voit donc la fin de fichier et ne bloque
        // pas.
        os.call_method1("dup2", (sauvegarde, 1)).expect("dup2");
        os.call_method1("close", (sauvegarde,)).expect("close");
        let octets: Vec<u8> = os
            .call_method1("read", (lecture, 65_536))
            .expect("read")
            .extract()
            .expect("octets");
        os.call_method1("close", (lecture,)).expect("close");
        String::from_utf8_lossy(&octets).into_owned()
    });

    (valeur, fuite)
}

#[test]
fn print_est_capture_et_ne_fuit_pas_sur_la_sortie_du_processus() {
    let banc = banc();
    let source =
        format!("print(\"{MARQUEUR}\")\nimport sys\nprint(\"vers stderr\", file=sys.stderr)\n");

    let (outcome, fuite) = sortie_du_processus_pendant(|| executer(&banc, &source));

    assert!(
        outcome.stdout.contains(MARQUEUR),
        "stdout capturé : {:?}",
        outcome.stdout
    );
    assert!(
        outcome.stderr.contains("vers stderr"),
        "stderr capturé : {:?}",
        outcome.stderr
    );
    assert!(
        !fuite.contains(MARQUEUR),
        "le marqueur a fui sur la sortie du processus : {fuite:?}"
    );
}

// --- 3. Trace d'appels ------------------------------------------------------

#[test]
fn une_exception_rend_la_trace_complete_et_garde_la_sortie_deja_ecrite() {
    let banc = banc();
    let outcome = banc
        .engine
        .run("print(\"avant\")\n1 / 0\n")
        .expect("le moteur accepte le script");

    let erreur = outcome.error.clone().expect("le script a échoué");
    assert!(erreur.contains("ZeroDivisionError"), "{erreur}");
    assert!(erreur.contains("Traceback"), "{erreur}");
    assert!(
        outcome.stdout.contains("avant"),
        "ce qui a été imprimé avant l'erreur est conservé : {:?}",
        outcome.stdout
    );
    assert!(!outcome.interrupted, "ce n'est pas une interruption");
}

// --- 4. `resolve` -----------------------------------------------------------

#[test]
fn resolve_rend_une_poignee_opaque() {
    let banc = banc();
    let outcome = executer(
        &banc,
        r#"
h = pyx.resolve("[Book1]Sheet1!A")
print(repr(h))
print(sorted(nom for nom in dir(h) if not nom.startswith("_")))
try:
    type(h)(1)
    print("CONSTRUCTIBLE")
except TypeError:
    print("NON-CONSTRUCTIBLE")
"#,
    );

    let lignes = lignes(&outcome);
    assert!(
        lignes[0].starts_with("Handle(") && lignes[0].ends_with(')'),
        "repr lisible attendu, obtenu {:?}",
        lignes[0]
    );
    assert_eq!(
        lignes[1], "[]",
        "aucun attribut public ne doit exposer l'entier : {:?}",
        lignes[1]
    );
    assert_eq!(
        lignes[2], "NON-CONSTRUCTIBLE",
        "un script ne doit pas pouvoir fabriquer une poignée"
    );
}

#[test]
fn resolve_leve_unknown_path_error_sur_un_chemin_inconnu_ou_invalide() {
    let banc = banc();
    let outcome = executer(
        &banc,
        r#"
for chemin in ("[Absent]Sheet1!A", "[Book1]Sheet1!ColonneQuiNexistePas", "[Book1", "", "!!"):
    try:
        pyx.resolve(chemin)
        print("RESOLU", chemin)
    except pyx.UnknownPathError:
        print("INCONNU", chemin)
    except Exception as erreur:
        print("AUTRE", type(erreur).__name__, chemin)
"#,
    );

    for ligne in lignes(&outcome) {
        assert!(
            ligne.starts_with("INCONNU"),
            "tout chemin fautif doit lever UnknownPathError : {ligne}"
        );
    }
}

// --- 5. `column` sans copie -------------------------------------------------

#[test]
fn column_est_une_vue_sans_copie_sur_le_buffer_du_modele() {
    let banc = banc();
    let attendue = adresse_rust(&banc.session, banc.a);

    let outcome = executer(
        &banc,
        r#"
v = pyx.column(pyx.resolve("[Book1]Sheet1!A"))
print([float(x) for x in v])
print(v.flags.writeable)
print(v.__array_interface__["data"][0])
"#,
    );

    let lignes = lignes(&outcome);
    assert_eq!(lignes[0], "[1.0, 2.0, 3.0]");
    assert_eq!(lignes[1], "False", "la vue doit être en lecture seule");
    let vue: usize = lignes[2].parse().expect("adresse entière");
    assert_eq!(
        vue, attendue,
        "numpy doit regarder EXACTEMENT le Vec du modèle : une copie silencieuse \
         rendrait les mêmes valeurs à une autre adresse"
    );
}

// --- 6. `column` refuse ce qu'il doit ---------------------------------------

#[test]
fn column_refuse_une_feuille_et_une_colonne_texte() {
    let banc = banc();
    let outcome = executer(
        &banc,
        r#"
for chemin in ("[Book1]Sheet1", "[Book1]Sheet1!T"):
    try:
        pyx.column(pyx.resolve(chemin))
        print("ACCEPTE", chemin)
    except pyx.InvalidValueError:
        print("REFUSE", chemin)
    except Exception as erreur:
        print("AUTRE", type(erreur).__name__, chemin)
"#,
    );

    assert_eq!(
        lignes(&outcome),
        vec![
            "REFUSE [Book1]Sheet1".to_owned(),
            "REFUSE [Book1]Sheet1!T".to_owned()
        ]
    );
}

// --- 7. `set_column` passe par le bus ---------------------------------------

#[test]
fn set_column_ecrit_par_le_bus() {
    let banc = banc();
    executer(
        &banc,
        r#"
a = pyx.resolve("[Book1]Sheet1!A")
b = pyx.resolve("[Book1]Sheet1!B")
pyx.set_column(b, pyx.column(a))
"#,
    );

    assert_eq!(valeurs(&banc.session, banc.b), vec![1.0, 2.0, 3.0]);
    assert!(
        banc.session
            .snapshot()
            .expect("instantané")
            .history
            .can_undo,
        "l'écriture doit avoir laissé une entrée d'historique"
    );
}

#[test]
fn set_column_refuse_une_longueur_differente_sans_envoyer_de_commande() {
    let banc = banc();
    let avant = revision(&banc.session);

    let outcome = executer(
        &banc,
        r#"
import numpy
b = pyx.resolve("[Book1]Sheet1!B")
for tableau in (numpy.array([1.0, 2.0]), numpy.array([1.0, 2.0, 3.0, 4.0])):
    try:
        pyx.set_column(b, tableau)
        print("ACCEPTE", len(tableau))
    except pyx.InvalidValueError:
        print("REFUSE", len(tableau))
"#,
    );

    assert_eq!(
        lignes(&outcome),
        vec!["REFUSE 2".to_owned(), "REFUSE 4".to_owned()]
    );
    assert_eq!(
        revision(&banc.session),
        avant,
        "un refus de longueur ne doit envoyer aucune commande"
    );
    assert_eq!(valeurs(&banc.session, banc.b), vec![10.0, 20.0, 30.0]);
}

#[test]
fn set_column_accepte_un_tableau_non_contigu() {
    let banc = banc();
    let outcome = executer(
        &banc,
        r#"
import numpy
source = numpy.arange(6.0)
tranche = source[::2]
print(tranche.flags["C_CONTIGUOUS"])
pyx.set_column(pyx.resolve("[Book1]Sheet1!B"), tranche)
"#,
    );

    assert_eq!(
        lignes(&outcome)[0],
        "False",
        "la tranche doit bien être non contiguë, sinon le test ne prouve rien"
    );
    assert_eq!(valeurs(&banc.session, banc.b), vec![0.0, 2.0, 4.0]);
}

// --- 8. `undo` depuis Python ------------------------------------------------

#[test]
fn undo_depuis_python_defait_les_ecritures_puis_rend_false() {
    let banc = banc();
    let outcome = executer(
        &banc,
        r#"
import numpy
a = pyx.resolve("[Book1]Sheet1!A")
b = pyx.resolve("[Book1]Sheet1!B")
pyx.set_column(b, pyx.column(a))
pyx.set_column(b, numpy.array([7.0, 8.0, 9.0]))
print([float(x) for x in pyx.column(b)])
print(pyx.undo())
print([float(x) for x in pyx.column(b)])
print(pyx.undo())
print([float(x) for x in pyx.column(b)])
print(pyx.undo())
"#,
    );

    assert_eq!(
        lignes(&outcome),
        vec![
            "[7.0, 8.0, 9.0]".to_owned(),
            "True".to_owned(),
            "[1.0, 2.0, 3.0]".to_owned(),
            "True".to_owned(),
            "[10.0, 20.0, 30.0]".to_owned(),
            // Pile vide : `False`, jamais une exception.
            "False".to_owned(),
        ]
    );
    assert_eq!(valeurs(&banc.session, banc.b), vec![10.0, 20.0, 30.0]);
}

// --- 9. Une seule pile ------------------------------------------------------

#[test]
fn une_seule_pile_dhistorique_pour_lacteur_et_le_script() {
    let banc = banc();
    // Une commande venue du côté Rust, comme le ferait une saisie de cellule.
    banc.session
        .execute(Command::SetCells {
            column: banc.a,
            start: 0,
            values: plotyx_core::ValueBlock::Float(vec![42.0]),
            expected_version: None,
        })
        .expect("commande de l'acteur");
    assert_eq!(valeurs(&banc.session, banc.a), vec![42.0, 2.0, 3.0]);

    let outcome = executer(&banc, "print(pyx.undo())\n");

    assert_eq!(lignes(&outcome), vec!["True".to_owned()]);
    assert_eq!(
        valeurs(&banc.session, banc.a),
        vec![1.0, 2.0, 3.0],
        "c'est bien la commande de l'acteur que le script a annulée"
    );
}

// --- 10. LE CRITÈRE DE FIN --------------------------------------------------

#[test]
fn critere_de_fin_lecture_sans_copie_ecriture_par_le_bus_annulation_par_ctrl_z() {
    let banc = banc();
    let arbre_avant = arbre(&banc.session);
    let adresse_de_a = adresse_rust(&banc.session, banc.a);

    // Un script lit A sans copie et écrit B par le bus.
    let outcome = executer(
        &banc,
        r#"
a = pyx.resolve("[Book1]Sheet1!A")
b = pyx.resolve("[Book1]Sheet1!B")
lu = pyx.column(a)
print(lu.__array_interface__["data"][0])
pyx.set_column(b, lu)
print([float(x) for x in pyx.column(b)])
"#,
    );

    let lignes = lignes(&outcome);
    let adresse_vue_par_numpy: usize = lignes[0].parse().expect("adresse entière");
    assert_eq!(
        adresse_vue_par_numpy, adresse_de_a,
        "la lecture doit être SANS COPIE : numpy regarde le Vec du modèle"
    );
    assert_eq!(lignes[1], "[1.0, 2.0, 3.0]");
    assert_eq!(valeurs(&banc.session, banc.b), vec![1.0, 2.0, 3.0]);

    // Ctrl+Z dans la fenêtre : une annulation venue du côté Rust.
    let annule = banc.session.undo().expect("undo de l'hôte");
    assert!(annule.is_some(), "il y avait bien quelque chose à annuler");

    assert_eq!(valeurs(&banc.session, banc.b), vec![10.0, 20.0, 30.0]);
    assert_eq!(
        arbre(&banc.session),
        arbre_avant,
        "après annulation, l'arbre doit être égal bit à bit à celui d'avant le script"
    );
}

// --- 11. Le GIL est relâché pendant l'attente de l'acteur -------------------

#[test]
fn le_gil_est_relache_pendant_lattente_de_lacteur() {
    // Accès exclusif : ce test déplace un réglage global de l'interpréteur.
    let banc = banc_exclusif();

    // Un intervalle de commutation énorme empêche l'interpréteur de lâcher le
    // GIL de lui-même entre deux instructions. Si un autre fil parvient quand
    // même à exécuter du Python pendant la boucle, c'est que le fil de script
    // l'a relâché EXPLICITEMENT — c'est-à-dire que `Python::detach` fait son
    // travail. Sans cette précaution, le test passerait tout seul : CPython
    // rend la main toutes les 5 ms, quoi que fasse le script.
    let source = r#"
import sys
ancien = sys.getswitchinterval()
sys.setswitchinterval(5.0)
try:
    b = pyx.resolve("[Book1]Sheet1!B")
    valeurs = pyx.column(pyx.resolve("[Book1]Sheet1!A"))
    for _ in range(500):
        pyx.set_column(b, valeurs)
finally:
    sys.setswitchinterval(ancien)
"#;

    let compteur = Arc::new(AtomicUsize::new(0));
    let temoin_compteur = Arc::clone(&compteur);
    let temoin_moteur = Arc::clone(&banc.engine);
    let temoin = thread::spawn(move || {
        if !attendre(Duration::from_secs(30), || temoin_moteur.is_running()) {
            return;
        }
        while temoin_moteur.is_running() {
            Python::attach(|py| {
                let _ = py.eval(c"1 + 1", None, None);
            });
            temoin_compteur.fetch_add(1, Ordering::Relaxed);
            // Sans cette pause, le fil témoin reprend le GIL aussitôt relâché
            // et met le fil de script en convoi : la boucle de 500 tours
            // passait de quelques dizaines de millisecondes à une vingtaine de
            // secondes. Le test mesure la POSSIBILITÉ de prendre le GIL, pas
            // la capacité à le monopoliser.
            thread::sleep(Duration::from_micros(200));
        }
    });

    let outcome = banc
        .engine
        .run(source)
        .expect("le moteur accepte le script");
    temoin.join().expect("le fil témoin se termine");

    assert!(
        outcome.error.is_none(),
        "script en échec :\n{}",
        outcome.error.clone().unwrap_or_default()
    );
    let tours = compteur.load(Ordering::Relaxed);
    assert!(
        tours >= 20,
        "un autre fil doit pouvoir exécuter du Python pendant la boucle : \
         {tours} tour(s) obtenu(s). Avec un intervalle de commutation de 5 s, \
         un fil de script qui garderait le GIL pendant ses allers-retours n'en \
         laisserait passer aucun."
    );
}

// --- 12. Interruption -------------------------------------------------------

/// Lance `source` sur un fil détaché et rend le canal qui portera son résultat.
///
/// Détaché, et non `thread::scope` : si un critère borné expire, il faut
/// pouvoir abandonner le fil plutôt que de bloquer le test sur sa jointure.
fn lancer_en_arriere_plan(
    moteur: &Arc<ScriptEngine>,
    source: &'static str,
) -> mpsc::Receiver<Result<ScriptOutcome, ScriptError>> {
    let (tx, rx) = mpsc::channel();
    let moteur = Arc::clone(moteur);
    thread::spawn(move || {
        let _ = tx.send(moteur.run(source));
    });
    rx
}

#[test]
fn interruption_dune_boucle_dappels_pyx() {
    let banc = banc();
    let resultat = lancer_en_arriere_plan(&banc.engine, "while True:\n    pyx.undo()\n");

    assert!(
        attendre(Duration::from_secs(30), || banc.engine.is_running()),
        "le script doit avoir démarré"
    );
    banc.engine.interrupt();

    let outcome = resultat
        .recv_timeout(Duration::from_secs(30))
        .expect("le script doit s'arrêter")
        .expect("le moteur avait accepté le script");
    assert!(outcome.interrupted, "l'arrêt doit être signalé comme voulu");
    assert!(
        outcome
            .error
            .as_deref()
            .is_some_and(|erreur| erreur.contains("KeyboardInterrupt")),
        "la trace doit dire où le script s'est arrêté : {:?}",
        outcome.error
    );
}

#[test]
fn interruption_dune_boucle_python_pure() {
    let banc = banc();
    let resultat = lancer_en_arriere_plan(&banc.engine, "while True:\n    pass\n");

    assert!(
        attendre(Duration::from_secs(30), || banc.engine.is_running()),
        "le script doit avoir démarré"
    );
    // Laisser la boucle entrer dans le `while` : l'exception asynchrone n'a
    // d'effet qu'à un point de contrôle de l'interpréteur.
    thread::sleep(Duration::from_millis(50));
    banc.engine.interrupt();

    let outcome = resultat
        .recv_timeout(Duration::from_secs(30))
        .expect("une boucle Python pure doit s'arrêter aussi")
        .expect("le moteur avait accepté le script");
    assert!(outcome.interrupted, "l'arrêt doit être signalé comme voulu");
    assert!(
        outcome
            .error
            .as_deref()
            .is_some_and(|erreur| erreur.contains("KeyboardInterrupt")),
        "la boucle doit avoir été cassée par un KeyboardInterrupt : {:?}",
        outcome.error
    );
}

// --- 13. Un script à la fois ------------------------------------------------

#[test]
fn un_seul_script_a_la_fois() {
    let banc = banc();
    let resultat = lancer_en_arriere_plan(&banc.engine, "while True:\n    pyx.undo()\n");

    assert!(
        attendre(Duration::from_secs(30), || banc.engine.is_running()),
        "le premier script doit avoir démarré"
    );
    let refus = banc.engine.run("pass");
    assert!(
        matches!(refus, Err(ScriptError::Busy)),
        "un second script doit être refusé : {refus:?}"
    );

    banc.engine.interrupt();
    let _ = resultat.recv_timeout(Duration::from_secs(30));
}

// --- 14. Le fil survit à un script en erreur --------------------------------

#[test]
fn le_fil_survit_a_un_script_en_erreur() {
    let banc = banc();
    let en_erreur = banc
        .engine
        .run("print(\"avant\")\n1 / 0\n")
        .expect("le moteur accepte le script");
    assert!(en_erreur.error.is_some());

    let ensuite = executer(
        &banc,
        "print([float(x) for x in pyx.column(pyx.resolve(\"[Book1]Sheet1!A\"))])\n",
    );
    assert_eq!(lignes(&ensuite), vec!["[1.0, 2.0, 3.0]".to_owned()]);
    assert!(!banc.engine.is_running());
}

// --- 15. Poignée et annulation ----------------------------------------------

#[test]
fn une_poignee_survit_a_une_suppression_annulee() {
    let banc = banc();
    // L'espace de noms est conservé d'une exécution à l'autre : la poignée
    // posée ici sera encore là au troisième script.
    executer(&banc, "h = pyx.resolve(\"[Book1]Sheet1!B\")\n");

    banc.session
        .execute(Command::DeleteColumn { column: banc.b })
        .expect("suppression côté Rust");

    let cassee = executer(
        &banc,
        r#"
try:
    pyx.column(h)
    print("ENCORE LA")
except pyx.BrokenReferenceError:
    print("CASSEE")
"#,
    );
    assert_eq!(lignes(&cassee), vec!["CASSEE".to_owned()]);

    banc.session.undo().expect("annulation de la suppression");

    let revenue = executer(&banc, "print([float(x) for x in pyx.column(h)])\n");
    assert_eq!(
        lignes(&revenue),
        vec!["[10.0, 20.0, 30.0]".to_owned()],
        "une poignée reste valide après une suppression annulée"
    );
}

// --- Compléments : la surface reste minuscule et documentée -----------------

#[test]
fn la_surface_expose_cinq_appels_et_quatre_erreurs() {
    // A2 (`guide-de-correction-robustesse.md` §F6) a ajouté une quatrième
    // exception, `ConcurrentWriteError` — ce test s'appelait
    // « ...et_trois_erreurs » et n'énumérait que les trois premières ; son
    // nom et sa liste attendue suivent le vocabulaire d'erreurs à jour.
    // L'import (issue #7) ajoute un cinquième appel ; la liste reste exacte
    // et sa documentation est éprouvée comme celle des appels du socle.
    let banc = banc();
    let outcome = executer(
        &banc,
        r#"
import pyx as importe
print(sorted(nom for nom in dir(importe) if not nom.startswith("_")))
print(importe.__doc__ is not None)
print(all(fonction.__doc__ for fonction in (importe.resolve, importe.column, importe.set_column, importe.undo, importe.import_file)))
"#,
    );

    let lignes = lignes(&outcome);
    assert_eq!(
        lignes[0],
        "['BrokenReferenceError', 'ConcurrentWriteError', 'Handle', 'InvalidValueError', \
         'UnknownPathError', 'column', 'import_file', 'resolve', 'set_column', 'undo']"
    );
    assert_eq!(lignes[1], "True", "le module est documenté");
    assert_eq!(lignes[2], "True", "les cinq appels sont documentés");
}

#[test]
fn la_surface_pyx_est_inutilisable_depuis_un_fil_cree_par_le_script() {
    let banc = banc();
    let outcome = executer(
        &banc,
        r#"
import threading
resultat = []
def travail():
    try:
        pyx.undo()
        resultat.append("PASSE")
    except RuntimeError:
        resultat.append("REFUSE")
fil = threading.Thread(target=travail)
fil.start()
fil.join()
print(resultat[0])
"#,
    );
    assert_eq!(lignes(&outcome), vec!["REFUSE".to_owned()]);
}

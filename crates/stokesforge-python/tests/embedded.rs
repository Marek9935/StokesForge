//! Tests d'intégration du pont Python embarqué.
//!
//! Ils ne s'exécutent qu'avec la feature `python` et exigent le préfixe
//! vendorisé : lance `scripts/python-vendor.sh` puis `scripts/python-test.sh`.
//!
//! CPython ne démarre qu'une fois par processus : tous les tests d'un même
//! binaire de test partagent donc **le même** interpréteur. [`prepare`] est le
//! point d'entrée unique, protégé par un `Once`, qui pose le piège
//! `PYTHONPATH` **avant** toute initialisation — ainsi la preuve d'isolation
//! vaut quel que soit l'ordre d'exécution des tests.

#![cfg(feature = "python")]

use std::path::PathBuf;
use std::sync::Once;

use stokesforge_python::interpreter::{self, EmbeddedPython};

/// Valeur de piège injectée dans `PYTHONPATH` : elle ne doit jamais apparaître
/// dans `sys.path`.
const PYTHONPATH_PIEGE: &str = "/tmp/piege-stokesforge-pythonpath";

/// Verrou de versions, lu à la compilation : une seule source de vérité.
const LOCK_ENV: &str = include_str!("../../../vendor/python/lock.env");

static PREPARE: Once = Once::new();

/// Lit une clé du verrou `vendor/python/lock.env`.
fn locked(key: &str) -> String {
    LOCK_ENV
        .lines()
        .filter_map(|line| line.split_once('='))
        .find(|(name, _)| name.trim() == key)
        .map(|(_, value)| value.trim().to_owned())
        .unwrap_or_else(|| panic!("clé {key} absente de vendor/python/lock.env"))
}

/// Résout le préfixe embarqué : `STOKESFORGE_PYTHON_HOME` si posée par le script de
/// test, sinon `vendor/python/extracted/python` relatif au manifeste.
fn home() -> PathBuf {
    match std::env::var_os(interpreter::HOME_ENV) {
        Some(value) => PathBuf::from(value),
        None => {
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../vendor/python/extracted/python")
        }
    }
}

/// Pose le piège d'environnement puis démarre l'interpréteur, une seule fois.
fn prepare() -> EmbeddedPython {
    let embedded = EmbeddedPython::with_home(home());
    PREPARE.call_once(|| {
        // Posé AVANT l'initialisation : c'est le seul moment où CPython
        // pourrait le lire. En mode isolé, il ne le lit pas.
        std::env::set_var("PYTHONPATH", PYTHONPATH_PIEGE);
        embedded
            .initialize()
            .expect("l'interpréteur embarqué doit démarrer");
    });
    embedded
}

/// #332 : l'appartenance au préfixe se juge par composants — ni un voisin
/// au nom prolongé, ni (sous Windows) une différence de séparateur.
#[test]
fn contains_path_compare_par_composants() {
    let embedded = EmbeddedPython::with_home(PathBuf::from("/opt/sf/python"));
    assert!(embedded.contains_path("/opt/sf/python"));
    assert!(embedded.contains_path("/opt/sf/python/lib/site-packages"));
    assert!(!embedded.contains_path("/opt/sf/python3/lib"));
    assert!(!embedded.contains_path("/usr/lib/python3.12"));

    #[cfg(windows)]
    {
        let embedded = EmbeddedPython::with_home(PathBuf::from("C:/dev/sf/python"));
        assert!(embedded.contains_path(r"C:\dev\sf\python\Lib\site-packages"));
        assert!(!embedded.contains_path(r"C:\dev\sf\python3\Lib"));
    }
}

#[test]
fn initialisation_idempotente() {
    let embedded = prepare();
    assert!(embedded.initialize().is_ok(), "deuxième appel");
    assert!(embedded.initialize().is_ok(), "troisième appel");
    assert!(EmbeddedPython::is_initialized());
}

#[test]
fn locate_refuse_un_prefixe_absent() {
    let bidon = EmbeddedPython::with_home(PathBuf::from("/n/existe/pas"));
    assert!(!bidon.home().join("lib").exists());
}

#[test]
fn version_python_verrouillee() {
    prepare();
    let version = interpreter::python_version().expect("version Python");
    let attendue = locked("PYTHON_VERSION");
    assert!(
        version.starts_with(&attendue),
        "version {version}, attendue {attendue}"
    );
}

#[test]
fn version_numpy_verrouillee() {
    prepare();
    let version = interpreter::numpy_version().expect("version numpy");
    assert_eq!(version, locked("NUMPY_VERSION"));
}

#[test]
fn aller_simple_somme() {
    prepare();
    let somme = interpreter::roundtrip_sum(&[1.0, 2.0, 3.0]).expect("roundtrip_sum");
    assert!((somme - 6.0).abs() < f64::EPSILON, "somme = {somme}");
}

#[test]
fn aller_retour_double() {
    prepare();
    let sortie = interpreter::roundtrip_vec(&[1.0, 2.0]).expect("roundtrip_vec");
    assert_eq!(sortie, vec![2.0, 4.0]);
}

#[test]
fn aller_retour_vide() {
    prepare();
    let sortie = interpreter::roundtrip_vec(&[]).expect("roundtrip_vec vide");
    assert!(sortie.is_empty());
}

#[test]
fn mode_isole_actif() {
    prepare();
    assert!(
        interpreter::sys_flags_isolated().expect("sys.flags.isolated"),
        "l'interpréteur doit tourner en mode isolé"
    );
}

#[test]
fn sys_path_ne_contient_que_le_prefixe_embarque() {
    let embedded = prepare();
    let prefixe = embedded
        .home()
        .canonicalize()
        .expect("le préfixe embarqué doit exister");
    // Comparaison de chemins canoniques, pas de chaînes : sous Windows,
    // `canonicalize` rend la forme `\\?\C:\…` alors que `sys.path` porte
    // `C:\…` (#332). Une entrée introuvable ne se canonicalise pas et reste
    // donc étrangère.
    let dans_le_prefixe = |chemin: &str| {
        std::path::Path::new(chemin)
            .canonicalize()
            .is_ok_and(|c| c.starts_with(&prefixe))
    };

    let chemins = interpreter::sys_path().expect("sys.path");
    assert!(!chemins.is_empty());
    for chemin in &chemins {
        assert!(
            !chemin.starts_with("/usr"),
            "sys.path contient un chemin système : {chemin}"
        );
        assert!(
            !chemin.contains("/.local/"),
            "sys.path contient un site utilisateur : {chemin}"
        );
        assert!(
            !chemin.contains("site-packages") || dans_le_prefixe(chemin),
            "sys.path contient un site-packages étranger : {chemin}"
        );
    }
}

#[test]
fn pythonpath_est_ignore() {
    prepare();
    let chemins = interpreter::sys_path().expect("sys.path");
    assert_eq!(
        std::env::var("PYTHONPATH").as_deref(),
        Ok(PYTHONPATH_PIEGE),
        "le piège doit bien être posé dans l'environnement du processus"
    );
    assert!(
        !chemins.iter().any(|c| c == PYTHONPATH_PIEGE),
        "PYTHONPATH a fui dans sys.path : {chemins:?}"
    );
}

#[test]
fn sys_prefix_pointe_sur_le_prefixe_embarque() {
    let embedded = prepare();
    let prefixe = interpreter::sys_prefix().expect("sys.prefix");
    let attendu = embedded.home().to_string_lossy().into_owned();
    assert_eq!(prefixe, attendu);
}

//! Tests d'intégration de l'aller-retour de tableaux Rust ↔ numpy (tâche 5.2).
//!
//! Ils ne s'exécutent qu'avec la feature `python` et exigent le préfixe
//! vendorisé : lance `scripts/python-vendor.sh` puis `scripts/python-test.sh`.
//!
//! Chaque test correspond à un critère d'acceptation du verrou. Deux d'entre
//! eux méritent d'être lus avant les autres :
//!
//! * [`emprunt_reel_pas_copie`] est le test décisif : il compare l'adresse du
//!   buffer vue par numpy à `Vec::as_ptr` — côté Python **et** côté Rust ;
//! * [`vue_perimee_reste_sur_l_ancien_buffer`] grave la politique retenue :
//!   une vue est un **instantané**, pas un abonnement.
//!
//! Comme dans `tests/embedded.rs`, CPython ne démarre qu'une fois par
//! processus : [`prepare`] est le point d'entrée unique, protégé par un `Once`.
//! Les deux binaires de test étant des processus distincts, cette amorce est
//! volontairement dupliquée plutôt que partagée par un module `common`.

#![cfg(feature = "python")]

use std::path::PathBuf;
use std::sync::{Arc, Once};

use numpy::{PyArray1, PyArrayMethods, PyUntypedArrayMethods};
use pyo3::prelude::*;
use pyo3::types::PyDict;
use stokesforge_python::array::{
    borrowed_readonly_array, copy_array_to_vec, measure_borrow_vs_copy, FloatBufferOwner,
};
use stokesforge_python::interpreter::{self, EmbeddedPython};

static PREPARE: Once = Once::new();

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

/// Démarre l'interpréteur embarqué, une seule fois pour tout le binaire.
fn prepare() {
    PREPARE.call_once(|| {
        EmbeddedPython::with_home(home())
            .initialize()
            .expect("l'interpréteur embarqué doit démarrer");
    });
}

/// Lit le contenu d'un tableau numpy de `f64` par l'emprunt partagé de
/// rust-numpy (jamais par `unsafe`).
fn values(array: &Bound<'_, PyArray1<f64>>) -> Vec<f64> {
    let readonly = array.try_readonly().expect("emprunt partagé");
    readonly.as_array().iter().copied().collect()
}

/// Adresse du buffer telle que **numpy** la publie, lue depuis Python.
fn python_data_pointer(array: &Bound<'_, PyArray1<f64>>) -> usize {
    array
        .getattr("__array_interface__")
        .expect("__array_interface__")
        .get_item("data")
        .expect("clé data")
        .get_item(0)
        .expect("adresse")
        .extract::<usize>()
        .expect("adresse entière")
}

/// Un dictionnaire de globales contenant `numpy`, pour les `eval` des tests.
fn numpy_globals<'py>(py: Python<'py>) -> Bound<'py, PyDict> {
    let globals = PyDict::new(py);
    globals
        .set_item("numpy", py.import("numpy").expect("import numpy"))
        .expect("set numpy");
    globals
}

// --- 1. Lecture correcte ----------------------------------------------------

#[test]
fn vue_rend_les_valeurs_et_la_longueur() {
    prepare();
    let data = Arc::new(vec![1.0, 2.0, 3.0]);
    Python::attach(|py| {
        let view = borrowed_readonly_array(py, &data).expect("emprunt");
        assert_eq!(view.len(), 3);
        assert_eq!(view.ndim(), 1);
        assert_eq!(values(&view), vec![1.0, 2.0, 3.0]);
    });
}

#[test]
fn vue_sur_un_buffer_vide() {
    prepare();
    let data: Arc<Vec<f64>> = Arc::new(Vec::new());
    Python::attach(|py| {
        let view = borrowed_readonly_array(py, &data).expect("emprunt vide");
        assert_eq!(view.len(), 0);
        assert!(values(&view).is_empty());
    });
}

// --- 2. Emprunt réel, pas copie (test décisif) ------------------------------

#[test]
fn emprunt_reel_pas_copie() {
    prepare();
    let data = Arc::new((0..1024).map(|index| index as f64).collect::<Vec<f64>>());
    let rust_pointer = data.as_ptr() as usize;

    Python::attach(|py| {
        let view = borrowed_readonly_array(py, &data).expect("emprunt");

        // Côté Rust : rust-numpy expose le pointeur de données du tableau.
        assert_eq!(
            view.data() as usize,
            rust_pointer,
            "le tableau numpy doit pointer EXACTEMENT sur le buffer du Vec"
        );
        // Côté Python : la même adresse, publiée par numpy lui-même.
        assert_eq!(
            python_data_pointer(&view),
            rust_pointer,
            "__array_interface__ doit publier l'adresse du buffer Rust"
        );
    });
}

// --- 3. `base` posé ---------------------------------------------------------

#[test]
fn base_est_un_float_buffer_owner() {
    prepare();
    let data = Arc::new(vec![1.0, 2.0, 3.0]);
    Python::attach(|py| {
        let view = borrowed_readonly_array(py, &data).expect("emprunt");
        let base = view.getattr("base").expect("attribut base");
        assert!(!base.is_none(), "base ne doit pas être None");

        let owner = base
            .cast::<FloatBufferOwner>()
            .expect("base doit être un FloatBufferOwner");
        assert_eq!(owner.get().len(), 3);
        assert!(!owner.get().is_empty());
    });
}

// --- 4. Lecture seule -------------------------------------------------------

#[test]
fn vue_en_lecture_seule() {
    prepare();
    let data = Arc::new(vec![1.0, 2.0, 3.0]);
    Python::attach(|py| {
        let view = borrowed_readonly_array(py, &data).expect("emprunt");

        let writeable = view
            .getattr("flags")
            .expect("flags")
            .getattr("writeable")
            .expect("writeable")
            .extract::<bool>()
            .expect("bool");
        assert!(!writeable, "arr.flags.writeable doit être faux");

        // Une écriture depuis Python lève une exception…
        let globals = PyDict::new(py);
        globals.set_item("arr", &view).expect("set arr");
        let erreur = py.run(c"arr[0] = 9.0", Some(&globals), None);
        assert!(erreur.is_err(), "arr[0] = 9.0 doit lever une exception");

        // …et le buffer Rust est inchangé.
        assert_eq!(data[0], 1.0);
        assert_eq!(values(&view), vec![1.0, 2.0, 3.0]);
    });
}

#[test]
fn le_drapeau_writeable_ne_peut_pas_etre_remis() {
    prepare();
    // `borrow_from_array` laisse OWNDATA à zéro : numpy refuse alors de rendre
    // le tableau réinscriptible. Ce n'est PAS ce qui garantit la sûreté (le
    // clone de l'Arc s'en charge), mais c'est un garde-fou de plus, et le
    // vérifier documente ce qu'un script peut ou ne peut pas faire.
    let data = Arc::new(vec![1.0, 2.0, 3.0]);
    Python::attach(|py| {
        let view = borrowed_readonly_array(py, &data).expect("emprunt");
        let globals = PyDict::new(py);
        globals.set_item("arr", &view).expect("set arr");
        let remise = py.run(c"arr.flags.writeable = True", Some(&globals), None);
        assert!(
            remise.is_err(),
            "un script ne doit pas pouvoir rendre la vue inscriptible"
        );
    });
}

// --- 5. Compteur de références ---------------------------------------------

#[test]
fn compteur_de_references_autour_de_la_vue() {
    prepare();
    let data = Arc::new(vec![1.0, 2.0, 3.0]);
    assert_eq!(Arc::strong_count(&data), 1, "avant l'emprunt");

    Python::attach(|py| {
        let view = borrowed_readonly_array(py, &data).expect("emprunt");
        assert!(
            Arc::strong_count(&data) >= 2,
            "pendant la vue, le compteur doit valoir au moins 2 (vu {})",
            Arc::strong_count(&data)
        );

        drop(view);
        // CPython compte les références immédiatement : la destruction du
        // tableau relâche `base`, donc le `FloatBufferOwner`, donc le clone de
        // l'Arc. `gc.collect()` n'est là que pour les cycles éventuels ; s'il
        // devenait nécessaire, ce serait le signe d'un cycle à documenter.
        py.import("gc")
            .expect("import gc")
            .call_method0("collect")
            .expect("gc.collect");

        assert_eq!(
            Arc::strong_count(&data),
            1,
            "après destruction de la vue, le compteur doit retomber à 1"
        );
    });
}

#[test]
fn deux_vues_coexistent_sur_le_meme_buffer() {
    prepare();
    let data = Arc::new(vec![1.0, 2.0, 3.0]);
    Python::attach(|py| {
        let une = borrowed_readonly_array(py, &data).expect("première vue");
        let deux = borrowed_readonly_array(py, &data).expect("seconde vue");

        // Même buffer, deux conteneurs distincts, donc compteur à 3.
        assert_eq!(une.data() as usize, deux.data() as usize);
        assert_eq!(Arc::strong_count(&data), 3);
        assert_eq!(values(&une), values(&deux));

        drop(une);
        assert_eq!(Arc::strong_count(&data), 2);
        drop(deux);
        assert_eq!(Arc::strong_count(&data), 1);
    });
}

// --- 6. Vue périmée : la politique retenue ---------------------------------

#[test]
fn vue_perimee_reste_sur_l_ancien_buffer() {
    prepare();
    let mut data = Arc::new(vec![1.0, 2.0, 3.0]);
    Python::attach(|py| {
        let ancienne = borrowed_readonly_array(py, &data).expect("emprunt");
        let ancien_pointeur = ancienne.data() as usize;

        // Écriture côté « modèle ». Le compteur vaut ≥ 2 : `make_mut` alloue un
        // buffer neuf et laisse l'ancien intact.
        Arc::make_mut(&mut data)[0] = 42.0;

        // La vue relit l'ANCIENNE valeur, sans planter. C'est le comportement
        // retenu : une vue est un instantané, pas un abonnement.
        assert_eq!(values(&ancienne), vec![1.0, 2.0, 3.0]);

        let nouvelle = borrowed_readonly_array(py, &data).expect("nouvel emprunt");
        assert_eq!(values(&nouvelle), vec![42.0, 2.0, 3.0]);

        assert_ne!(
            ancien_pointeur,
            nouvelle.data() as usize,
            "make_mut avec un compteur ≥ 2 doit avoir alloué un buffer neuf"
        );
    });
}

// --- 7. Croissance sous la vue ---------------------------------------------

#[test]
fn croissance_sous_la_vue() {
    prepare();
    let mut data = Arc::new(vec![1.0, 2.0, 3.0]);
    Python::attach(|py| {
        let vue = borrowed_readonly_array(py, &data).expect("emprunt");
        let ancien_pointeur = vue.data() as usize;

        // Ajout de lignes : sans le clone de l'Arc, le Vec grandirait en place
        // et la réallocation laisserait la vue sur de la mémoire libérée.
        Arc::make_mut(&mut data).extend([4.0, 5.0, 6.0, 7.0, 8.0]);

        assert_eq!(vue.len(), 3, "la vue garde sa longueur d'origine");
        assert_eq!(values(&vue), vec![1.0, 2.0, 3.0]);
        assert_eq!(data.len(), 8);
        assert_ne!(ancien_pointeur, data.as_ptr() as usize);
    });
}

// --- 8. Survie au relâchement côté Rust ------------------------------------

#[test]
fn la_vue_survit_au_drop_cote_rust() {
    prepare();
    let data = Arc::new(vec![1.0, 2.0, 3.0]);
    Python::attach(|py| {
        let vue = borrowed_readonly_array(py, &data).expect("emprunt");
        let pointeur = vue.data() as usize;

        // Le conteneur détient désormais la dernière référence : c'est la
        // preuve directe que `base` fait son travail.
        drop(data);

        assert_eq!(values(&vue), vec![1.0, 2.0, 3.0]);
        assert_eq!(vue.data() as usize, pointeur, "le buffer n'a pas bougé");
    });
}

// --- 9. Retour : contigu, non contigu, et les refus -------------------------

#[test]
fn retour_non_contigu() {
    prepare();
    Python::attach(|py| {
        let globals = numpy_globals(py);
        let tranche = py
            .eval(
                c"numpy.arange(10, dtype=numpy.float64)[::2]",
                Some(&globals),
                None,
            )
            .expect("eval");
        assert_eq!(
            copy_array_to_vec(&tranche).expect("copie non contiguë"),
            vec![0.0, 2.0, 4.0, 6.0, 8.0]
        );
    });
}

#[test]
fn retour_contigu() {
    prepare();
    Python::attach(|py| {
        let globals = numpy_globals(py);
        let tableau = py
            .eval(
                c"numpy.arange(5, dtype=numpy.float64)",
                Some(&globals),
                None,
            )
            .expect("eval");
        assert_eq!(
            copy_array_to_vec(&tableau).expect("copie contiguë"),
            vec![0.0, 1.0, 2.0, 3.0, 4.0]
        );
    });
}

#[test]
fn retour_inverse() {
    prepare();
    Python::attach(|py| {
        let globals = numpy_globals(py);
        let tableau = py
            .eval(
                c"numpy.arange(4, dtype=numpy.float64)[::-1]",
                Some(&globals),
                None,
            )
            .expect("eval");
        assert_eq!(
            copy_array_to_vec(&tableau).expect("copie à pas négatif"),
            vec![3.0, 2.0, 1.0, 0.0]
        );
    });
}

#[test]
fn retour_refuse_un_int64() {
    prepare();
    Python::attach(|py| {
        let globals = numpy_globals(py);
        let tableau = py
            .eval(c"numpy.arange(4, dtype=numpy.int64)", Some(&globals), None)
            .expect("eval");
        let message = copy_array_to_vec(&tableau)
            .expect_err("un int64 doit être refusé")
            .to_string();
        assert!(
            message.contains("float64") && message.contains("int64"),
            "message peu explicite : {message}"
        );
    });
}

#[test]
fn retour_refuse_deux_dimensions() {
    prepare();
    Python::attach(|py| {
        let globals = numpy_globals(py);
        let tableau = py
            .eval(
                c"numpy.zeros((2, 3), dtype=numpy.float64)",
                Some(&globals),
                None,
            )
            .expect("eval");
        let message = copy_array_to_vec(&tableau)
            .expect_err("un tableau 2-D doit être refusé")
            .to_string();
        assert!(
            message.contains("2 dimensions"),
            "message peu explicite : {message}"
        );
    });
}

#[test]
fn retour_refuse_une_liste_python() {
    prepare();
    Python::attach(|py| {
        let liste = py.eval(c"[1.0, 2.0, 3.0]", None, None).expect("eval");
        let message = copy_array_to_vec(&liste)
            .expect_err("une liste doit être refusée")
            .to_string();
        assert!(
            message.contains("list") && message.contains("numpy"),
            "message peu explicite : {message}"
        );
    });
}

// --- 10. Aller-retour complet, bit à bit ------------------------------------

#[test]
fn aller_retour_bit_a_bit() {
    prepare();
    let mut valeurs: Vec<f64> = (0..1000).map(|index| index as f64 * 0.5).collect();
    valeurs[7] = f64::NAN;
    valeurs[9] = -0.0;
    valeurs[11] = f64::INFINITY;
    valeurs[13] = f64::NEG_INFINITY;
    valeurs[17] = f64::MIN_POSITIVE / 3.0; // sous-normal
    let data = Arc::new(valeurs);

    Python::attach(|py| {
        let vue = borrowed_readonly_array(py, &data).expect("emprunt");
        let globals = numpy_globals(py);
        globals.set_item("vue", &vue).expect("set vue");
        let copie = py
            .eval(c"numpy.copy(vue)", Some(&globals), None)
            .expect("numpy.copy");

        let retour = copy_array_to_vec(&copie).expect("retour");
        assert_eq!(retour.len(), data.len());
        // Comparaison bit à bit : `NaN != NaN` en IEEE, et `-0.0 == 0.0`.
        for (index, (avant, apres)) in data.iter().zip(retour.iter()).enumerate() {
            assert_eq!(
                avant.to_bits(),
                apres.to_bits(),
                "valeur {index} altérée par l'aller-retour"
            );
        }
    });
}

// --- 11. Mesure -------------------------------------------------------------

#[test]
fn emprunt_au_moins_vingt_fois_plus_rapide_que_la_copie() {
    prepare();
    const LEN: usize = 1_000_000;
    let (borrow_ns, copy_ns) = measure_borrow_vs_copy(LEN).expect("mesure");

    println!("emprunt de {LEN} f64 : {borrow_ns} ns");
    println!("copie   de {LEN} f64 : {copy_ns} ns");
    println!("rapport : ×{:.1}", copy_ns as f64 / borrow_ns.max(1) as f64);

    assert!(
        borrow_ns > 0,
        "mesure d'emprunt nulle : horloge trop grossière"
    );

    // `scripts/python-memcheck.sh` pose STOKESFORGE_UNDER_VALGRIND=1 avant de
    // lancer ce binaire sous valgrind. Sous instrumentation, tout est
    // ralenti, mais PAS uniformément : l'emprunt est dominé par quelques
    // appels (allocation du FloatBufferOwner, construction du ndarray), que
    // valgrind pénalise proportionnellement bien plus qu'un déroulé linéaire ;
    // la copie, elle, est dominée par un transfert mémoire, que valgrind
    // ralentit d'un facteur beaucoup plus modeste. Le RAPPORT mesuré sous
    // instrumentation n'a donc aucun sens et ferait échouer ce test sur un
    // faux négatif (constaté : ×9 sous valgrind contre ×240 à ×370 en
    // exécution ordinaire). On se contente alors d'afficher les deux mesures,
    // sans affirmer le rapport. Hors instrumentation, l'assertion reste
    // active : c'est elle qui protège le zéro-copie contre une régression
    // silencieuse en copie.
    if std::env::var_os("STOKESFORGE_UNDER_VALGRIND").is_some() {
        println!(
            "STOKESFORGE_UNDER_VALGRIND posée : rapport non affirmé (valgrind ralentit \
             l'emprunt et la copie de façon non uniforme, le rapport mesuré ici \
             n'a pas de sens)"
        );
        return;
    }

    assert!(
        copy_ns >= borrow_ns * 20,
        "l'emprunt doit être au moins 20× plus rapide : emprunt {borrow_ns} ns, copie {copy_ns} ns"
    );
}

//! Batterie 7 — la frontière Rust ↔ Python : conversions de types, remontée
//! des erreurs, et ce qui traverse en silence.
//!
//! Nécessite la feature `python` et l'interpréteur vendorisé (voir
//! `scripts/python-test.sh` pour les trois variables à poser).
#![cfg(feature = "python")]

use plotyx_commands::Command;
use plotyx_core::{Column, ColumnType, ColumnValues, NodeId, Project, ValueBlock};
use plotyx_python::interpreter::EmbeddedPython;
use plotyx_script::ScriptEngine;
use plotyx_session::{Session, SessionHandle};
use std::sync::OnceLock;

/// Un moteur unique pour tout le fichier : CPython ne démarre qu'une fois par
/// processus, et l'espace de noms d'un script persiste d'une exécution à
/// l'autre (documenté).
struct Banc {
    _session: Session,
    handle: SessionHandle,
    engine: ScriptEngine,
    colonnes: Vec<NodeId>,
    #[allow(dead_code)]
    sheet: NodeId,
}

static BANC: OnceLock<std::sync::Mutex<Banc>> = OnceLock::new();

fn banc() -> &'static std::sync::Mutex<Banc> {
    BANC.get_or_init(|| {
        let mut p = Project::new("PY").unwrap();
        let root = p.root();
        p.add_workbook(root, Some("Book1")).unwrap();
        let sheet = p.resolve_path("[Book1]Sheet1").unwrap();
        let mut colonnes = Vec::new();
        for nom in ["A", "B"] {
            colonnes.push(
                p.add_column(sheet, None, Some(nom), Column::from_f64(&[1.0, 2.0, 3.0, 4.0, 5.0]))
                    .unwrap(),
            );
        }
        // Une colonne texte, pour les refus de type.
        colonnes.push(p.add_column(sheet, None, Some("T"), Column::new(ColumnType::Text)).unwrap());
        let session = Session::spawn(p, None).unwrap();
        let handle = session.handle();
        let python = EmbeddedPython::locate().expect(
            "interpréteur vendorisé introuvable : poser PLOTYX_PYTHON_HOME (cf. scripts/python-test.sh)",
        );
        let engine = ScriptEngine::spawn(handle.clone(), python).expect("démarrage du moteur");
        std::sync::Mutex::new(Banc { _session: session, handle, engine, colonnes, sheet })
    })
}

/// Exécute un script et rend (stdout, stderr, erreur formatée).
fn run(source: &str) -> (String, String, Option<String>) {
    let b = banc().lock().unwrap();
    let out = b
        .engine
        .run(source)
        .expect("le moteur doit accepter le travail");
    (out.stdout, out.stderr, out.error)
}

/// Identifiants des colonnes du banc, copiés une fois. Les relire à travers le
/// verrou depuis une expression qui le tient déjà s'auto-bloquerait :
/// `std::sync::Mutex` n'est pas réentrant.
fn colonne_a() -> NodeId {
    banc().lock().unwrap().colonnes[0]
}

fn valeurs(col: NodeId) -> Vec<f64> {
    let b = banc().lock().unwrap();
    match b
        .handle
        .snapshot()
        .unwrap()
        .tree
        .node(col)
        .unwrap()
        .as_column()
        .unwrap()
        .values()
    {
        ColumnValues::Float(v) => v.as_ref().clone(),
        _ => panic!("colonne texte"),
    }
}

fn remettre_a_zero() {
    let b = banc().lock().unwrap();
    let col = b.colonnes[0];
    b.handle
        .execute(Command::SetCells {
            column: col,
            start: 0,
            values: ValueBlock::Float(vec![1.0, 2.0, 3.0, 4.0, 5.0]),
        })
        .unwrap();
}

// ============================================================== conversions

#[test]
fn b7_types_refuses_par_set_column() {
    remettre_a_zero();
    let cas: &[(&str, &str)] = &[
        ("une liste Python", "[1.0, 2.0, 3.0, 4.0, 5.0]"),
        ("un entier", "42"),
        ("une chaîne", "'abcde'"),
        ("None", "None"),
        ("un tableau int64", "np.arange(5)"),
        ("un tableau float32", "np.zeros(5, dtype=np.float32)"),
        ("un tableau 2-D", "np.zeros((1, 5))"),
        ("un scalaire numpy", "np.float64(1.0)"),
        ("un tableau 0-D", "np.array(1.0)"),
        ("un tableau complexe", "np.zeros(5, dtype=np.complex128)"),
        ("un tableau d'objets", "np.array([1.0]*5, dtype=object)"),
        ("un tableau de longueur 4", "np.zeros(4)"),
        ("un tableau de longueur 6", "np.zeros(6)"),
    ];
    let mut acceptes = Vec::new();
    for (nom, expr) in cas {
        let src = format!(
            "import numpy as np\n\
             h = pyx.resolve('[Book1]Sheet1!A')\n\
             try:\n    pyx.set_column(h, {expr})\n    print('ACCEPTÉ')\n\
             except Exception as e:\n    print(type(e).__name__)\n"
        );
        let (out, _err, erreur) = run(&src);
        assert!(
            erreur.is_none(),
            "{nom} : le script lui-même a échoué : {erreur:?}"
        );
        let verdict = out.trim().to_string();
        println!("{nom:30} -> {verdict}");
        if verdict == "ACCEPTÉ" {
            acceptes.push((*nom, valeurs(colonne_a())));
        }
        remettre_a_zero();
    }
    assert!(
        acceptes.is_empty(),
        "des types qui ne sont pas un tableau numpy float64 1-D ont été acceptés : {acceptes:?}"
    );
}

#[test]
fn b7_ordre_des_octets_dun_tableau_big_endian() {
    // Un tableau '>f8' a le bon dtype « float64 » mais l'ordre inverse. S'il
    // passe le contrôle, ses octets sont relus tels quels : corruption muette.
    remettre_a_zero();
    let src = "import numpy as np\n\
        h = pyx.resolve('[Book1]Sheet1!A')\n\
        src = np.array([1.0, 2.0, 3.0, 4.0, 5.0])\n\
        be = src.astype('>f8')\n\
        print('dtype', be.dtype, 'valeurs Python', list(be))\n\
        try:\n    pyx.set_column(h, be)\n    print('ACCEPTÉ')\n\
        except Exception as e:\n    print(type(e).__name__, e)\n";
    let (out, _err, erreur) = run(src);
    println!("{out}");
    assert!(erreur.is_none(), "{erreur:?}");
    if out.contains("ACCEPTÉ") {
        let v = valeurs(colonne_a());
        assert_eq!(
            v,
            vec![1.0, 2.0, 3.0, 4.0, 5.0],
            "un tableau big-endian a été accepté et relu à l'envers : le modèle \
             contient {v:?} au lieu des valeurs que Python affichait"
        );
    }
    remettre_a_zero();
}

#[test]
fn b7_tableau_masque_perd_son_masque() {
    remettre_a_zero();
    let src = "import numpy as np\n\
        h = pyx.resolve('[Book1]Sheet1!A')\n\
        m = np.ma.masked_array([1.0, 2.0, 3.0, 4.0, 5.0], mask=[0,1,0,1,0], fill_value=-999.0)\n\
        print('vu de Python :', m.filled())\n\
        try:\n    pyx.set_column(h, m)\n    print('ACCEPTÉ')\n\
        except Exception as e:\n    print(type(e).__name__)\n";
    let (out, _err, erreur) = run(src);
    println!("{out}");
    assert!(erreur.is_none(), "{erreur:?}");
    if out.contains("ACCEPTÉ") {
        let v = valeurs(colonne_a());
        assert!(
            v[1].is_nan() && v[3].is_nan(),
            "un tableau masqué a été accepté et son masque ignoré : le modèle \
             contient {v:?}, les cases masquées ne sont pas manquantes"
        );
    }
    remettre_a_zero();
}

#[test]
fn b7_tableaux_non_contigus_sont_lus_correctement() {
    remettre_a_zero();
    let src = "import numpy as np\n\
        h = pyx.resolve('[Book1]Sheet1!A')\n\
        base = np.arange(10, dtype=np.float64)\n\
        pyx.set_column(h, base[::2])          # pas de 2\n\
        print('pas2 ok')\n";
    let (out, _err, erreur) = run(src);
    assert!(erreur.is_none(), "{out} / {erreur:?}");
    assert_eq!(valeurs(colonne_a()), vec![0.0, 2.0, 4.0, 6.0, 8.0]);

    let src = "import numpy as np\n\
        h = pyx.resolve('[Book1]Sheet1!A')\n\
        pyx.set_column(h, np.arange(5, dtype=np.float64)[::-1])   # pas négatif\n";
    let (_out, _err, erreur) = run(src);
    assert!(erreur.is_none(), "{erreur:?}");
    assert_eq!(valeurs(colonne_a()), vec![4.0, 3.0, 2.0, 1.0, 0.0]);
    remettre_a_zero();
}

#[test]
fn b7_valeurs_speciales_traversent_fidelement() {
    remettre_a_zero();
    let src = "import numpy as np\n\
        h = pyx.resolve('[Book1]Sheet1!A')\n\
        v = np.array([np.nan, np.inf, -np.inf, -0.0, 5e-324])\n\
        pyx.set_column(h, v)\n";
    let (_out, _err, erreur) = run(src);
    assert!(erreur.is_none(), "{erreur:?}");
    let v = valeurs(colonne_a());
    assert!(v[0].is_nan(), "NaN perdu : {v:?}");
    assert_eq!(v[1], f64::INFINITY);
    assert_eq!(v[2], f64::NEG_INFINITY);
    assert_eq!(
        v[3].to_bits(),
        (-0.0f64).to_bits(),
        "le zéro négatif est devenu {} (bits {:x})",
        v[3],
        v[3].to_bits()
    );
    assert_eq!(v[4], 5e-324, "le dénormal minimal est devenu {}", v[4]);
    remettre_a_zero();
}

#[test]
fn b7_vue_sans_copie_pointe_bien_sur_le_modele() {
    remettre_a_zero();
    let src = "import numpy as np\n\
        h = pyx.resolve('[Book1]Sheet1!A')\n\
        v = pyx.column(h)\n\
        print('writeable', v.flags.writeable, 'owndata', v.flags.owndata)\n\
        print('base', type(v.base).__name__)\n\
        print('valeurs', list(v))\n\
        try:\n    v[0] = 99.0\n    print('ÉCRITURE ACCEPTÉE')\n\
        except ValueError as e:\n    print('écriture refusée')\n";
    let (out, _err, erreur) = run(src);
    println!("{out}");
    assert!(erreur.is_none(), "{erreur:?}");
    assert!(
        out.contains("writeable False"),
        "la vue est modifiable : {out}"
    );
    assert!(out.contains("écriture refusée"), "{out}");
    assert!(
        !valeurs(colonne_a()).contains(&99.0),
        "le modèle a été modifié à travers la vue en lecture seule"
    );
    remettre_a_zero();
}

#[test]
fn b7_vue_conservee_pendant_une_ecriture_ne_corrompt_pas() {
    // Contrat documenté : la vue reste sur l'ancien buffer (instantané périmé),
    // JAMAIS un accès invalide. On le vérifie sur un gros buffer, pour que la
    // réallocation soit certaine si elle a lieu.
    remettre_a_zero();
    let src = "import numpy as np\n\
        h = pyx.resolve('[Book1]Sheet1!B')\n\
        pyx.set_column(h, np.arange(5, dtype=np.float64))\n\
        vue = pyx.column(h)\n\
        avant = vue.copy()\n\
        for _ in range(200):\n\
        \x20   pyx.set_column(h, np.random.default_rng(0).random(5))\n\
        apres = np.asarray(vue)\n\
        print('vue stable :', bool(np.array_equal(avant, apres)))\n\
        print('vue relue  :', list(pyx.column(h))[:2])\n\
        print('somme vue  :', float(np.sum(vue)))\n";
    let (out, _err, erreur) = run(src);
    println!("{out}");
    assert!(erreur.is_none(), "{erreur:?}");
    assert!(
        out.contains("vue stable : True"),
        "la vue numpy a changé sous les pieds du script : {out}"
    );
}

#[test]
fn b7_erreurs_de_lhote_deviennent_les_trois_exceptions_annoncees() {
    let src = "import numpy as np\n\
        def essai(f):\n\
        \x20   try:\n\
        \x20       f()\n\
        \x20       return 'AUCUNE'\n\
        \x20   except Exception as e:\n\
        \x20       return type(e).__name__\n\
        print('chemin absurde     :', essai(lambda: pyx.resolve('[[[')))\n\
        print('chemin vide        :', essai(lambda: pyx.resolve('')))\n\
        print('objet inexistant   :', essai(lambda: pyx.resolve('[Nope]S!C')))\n\
        print('feuille comme col  :', essai(lambda: pyx.column(pyx.resolve('[Book1]Sheet1'))))\n\
        print('colonne texte      :', essai(lambda: pyx.column(pyx.resolve('[Book1]Sheet1!T'))))\n\
        print('classeur comme col :', essai(lambda: pyx.column(pyx.resolve('[Book1]'))))\n";
    let (out, _err, erreur) = run(src);
    println!("{out}");
    assert!(erreur.is_none(), "{erreur:?}");
    for ligne in out.lines() {
        assert!(
            ligne.contains("UnknownPathError")
                || ligne.contains("BrokenReferenceError")
                || ligne.contains("InvalidValueError"),
            "une erreur de l'hôte n'est pas l'une des trois exceptions annoncées : {ligne}"
        );
    }
}

#[test]
fn b7_poignee_est_opaque_et_infabricable() {
    let src = "\
        h = pyx.resolve('[Book1]Sheet1!A')\n\
        print('repr', repr(h))\n\
        print('attributs', [a for a in dir(h) if not a.startswith('__')])\n\
        try:\n    pyx.Handle()\n    print('CONSTRUCTIBLE')\n\
        except TypeError:\n    print('non constructible')\n\
        import pickle\n\
        try:\n    pickle.dumps(h)\n    print('PICKLABLE')\n\
        except Exception as e:\n    print('non picklable', type(e).__name__)\n";
    let (out, _err, erreur) = run(src);
    println!("{out}");
    assert!(erreur.is_none(), "{erreur:?}");
    assert!(
        !out.contains("CONSTRUCTIBLE"),
        "une poignée peut être fabriquée : {out}"
    );
    assert!(
        out.contains("attributs []"),
        "la poignée expose des attributs : {out}"
    );
}

#[test]
fn b7_surface_inutilisable_depuis_un_fil_python() {
    let src = "import threading\n\
        resultat = []\n\
        def travail():\n\
        \x20   try:\n\
        \x20       resultat.append(('ok', pyx.resolve('[Book1]Sheet1!A')))\n\
        \x20   except Exception as e:\n\
        \x20       resultat.append((type(e).__name__, str(e)[:60]))\n\
        t = threading.Thread(target=travail)\n\
        t.start(); t.join()\n\
        print(resultat[0][0])\n";
    let (out, _err, erreur) = run(src);
    println!("depuis un fil Python : {out}");
    assert!(erreur.is_none(), "{erreur:?}");
    assert!(
        !out.trim().starts_with("ok"),
        "la surface pyx est utilisable depuis un fil créé par le script : {out}"
    );
    assert!(out.contains("RuntimeError"), "message inattendu : {out}");
}

#[test]
fn b7_undo_depuis_python_est_bien_la_pile_de_lhote() {
    remettre_a_zero();
    let src = "import numpy as np\n\
        h = pyx.resolve('[Book1]Sheet1!A')\n\
        pyx.set_column(h, np.array([10.0, 20.0, 30.0, 40.0, 50.0]))\n\
        print('après écriture', list(pyx.column(h)))\n\
        print('undo', pyx.undo())\n\
        print('après undo   ', list(pyx.column(h)))\n";
    let (out, _err, erreur) = run(src);
    println!("{out}");
    assert!(erreur.is_none(), "{erreur:?}");
    let v = valeurs(colonne_a());
    assert_eq!(
        v,
        vec![1.0, 2.0, 3.0, 4.0, 5.0],
        "pyx.undo() n'a pas défait l'écriture"
    );
    remettre_a_zero();
}

#[test]
fn b7_source_avec_octet_nul_et_unicode() {
    let (out, _err, erreur) = run("print('\u{e9}\u{1f980}')");
    assert!(erreur.is_none(), "{erreur:?}");
    assert_eq!(out.trim(), "é🦀");
    // Un octet nul dans la source : refus documenté, pas de panique.
    let out = {
        let b = banc().lock().unwrap();
        b.engine
            .run("print('a\0b')")
            .expect("le moteur accepte le travail")
    };
    assert!(
        out.error.is_some(),
        "une source contenant un octet nul a été exécutée"
    );
    println!(
        "octet nul -> {}",
        out.error.unwrap().lines().last().unwrap_or("")
    );
}

#[test]
fn b7_exception_python_remonte_avec_sa_trace() {
    let (_out, _err, erreur) = run("def f():\n    raise ValueError('boum')\nf()\n");
    let e = erreur.expect("une exception doit remonter");
    assert!(e.contains("ValueError"), "{e}");
    assert!(e.contains("boum"), "{e}");
    assert!(e.contains("Traceback"), "trace absente : {e}");
}

#[test]
fn b7_recursion_profonde_cote_python() {
    let (_out, _err, erreur) = run("def f(n):\n    return f(n+1)\ntry:\n    f(0)\nexcept RecursionError:\n    print('RecursionError')\n");
    assert!(
        erreur.is_none(),
        "la récursion infinie n'a pas été rattrapée : {erreur:?}"
    );
}

#[test]
fn b7_sortie_massive_ne_fait_pas_exploser_la_capture() {
    let (out, _err, erreur) = run("for i in range(20000):\n    print('x' * 40)\n");
    assert!(erreur.is_none(), "{erreur:?}");
    println!("sortie capturée : {} octets", out.len());
    assert!(out.len() > 100, "la sortie a disparu");
}

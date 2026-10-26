//! Batterie 9 — le fil de script sous contrainte : corruption par tableau
//! masqué, course avec l'interface, interruption, et pressions diverses.
#![cfg(feature = "python")]

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, OnceLock};
use stokesforge_commands::Command;
use stokesforge_core::{Column, ColumnType, ColumnValues, NodeId, Project, ValueBlock};
use stokesforge_python::interpreter::EmbeddedPython;
use stokesforge_script::{ScriptEngine, ScriptError};
use stokesforge_session::{Session, SessionHandle};

struct Banc {
    _session: Session,
    handle: SessionHandle,
    engine: ScriptEngine,
    a: NodeId,
    b: NodeId,
    sheet: NodeId,
}

static BANC: OnceLock<std::sync::Mutex<Banc>> = OnceLock::new();

fn banc() -> &'static std::sync::Mutex<Banc> {
    BANC.get_or_init(|| {
        let mut p = Project::new("PY").unwrap();
        let root = p.root();
        p.add_workbook(root, Some("Book1")).unwrap();
        let sheet = p.resolve_path("[Book1]Sheet1").unwrap();
        let a = p
            .add_column(
                sheet,
                None,
                Some("A"),
                Column::from_f64(&[1.0, 2.0, 3.0, 4.0, 5.0]),
            )
            .unwrap();
        let b = p
            .add_column(sheet, None, Some("B"), Column::from_f64(&vec![0.0; 1000]))
            .unwrap();
        p.add_column(sheet, None, Some("Jetable"), Column::from_f64(&[1.0]))
            .unwrap();
        let session = Session::spawn(p, None).unwrap();
        let handle = session.handle();
        let python = EmbeddedPython::locate().expect("interpréteur vendorisé introuvable");
        let engine = ScriptEngine::spawn(handle.clone(), python).expect("démarrage du moteur");
        std::sync::Mutex::new(Banc {
            _session: session,
            handle,
            engine,
            a,
            b,
            sheet,
        })
    })
}

fn run(source: &str) -> stokesforge_script::ScriptOutcome {
    let g = banc().lock().unwrap();
    g.engine.run(source).expect("le moteur accepte le travail")
}

fn ids() -> (NodeId, NodeId, NodeId) {
    let g = banc().lock().unwrap();
    (g.a, g.b, g.sheet)
}

fn poignee() -> SessionHandle {
    banc().lock().unwrap().handle.clone()
}

fn valeurs(col: NodeId) -> Vec<f64> {
    match poignee()
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

fn reinit(col: NodeId, v: Vec<f64>) {
    poignee()
        .execute(Command::SetCells {
            column: col,
            start: 0,
            values: ValueBlock::Float(v),
            expected_version: None,
        })
        .unwrap();
}

// ======================================================== tableau masqué

#[test]
fn b9_tableau_masque_est_refuse_plutot_que_decouvert() {
    // A1 (`guide-de-correction-robustesse.md` §F1) — décidé : une valeur non
    // représentable dans la convention du modèle est REFUSÉE, jamais
    // convertie en silence. Un `np.ma.masked_array` qui porte au moins une
    // valeur masquée est donc refusé par `pyst.set_column`, y compris ici où
    // les positions masquées coïncident déjà avec des `NaN` — l'ancienne
    // version de ce test acceptait l'écriture (elle vérifiait seulement que
    // le résultat, une fois accepté, contenait bien des `NaN`), ce qui n'est
    // plus le comportement voulu depuis A1.
    let (a, _b, _s) = ids();
    reinit(a, vec![1.0, 2.0, 3.0, 4.0, 5.0]);
    let out = run("import numpy as np\n\
         h = pyst.resolve('[Book1]Sheet1!A')\n\
         mesures = np.ma.masked_invalid(np.array([10.0, np.nan, 30.0, np.nan, 50.0]))\n\
         print('ce que Python voit  :', list(mesures.filled(np.nan)))\n\
         print('données sous masque :', list(mesures.data))\n\
         try:\n\
         \x20   pyst.set_column(h, mesures)\n\
         \x20   print('refus: non')\n\
         except pyst.InvalidValueError as exc:\n\
         \x20   print('refus: oui', exc)\n");
    assert!(out.error.is_none(), "{:?}", out.error);
    println!("{}", out.stdout);
    assert!(
        out.stdout.contains("refus: oui"),
        "un tableau masqué doit être refusé, même quand les positions masquées \
         coïncident avec des NaN déjà présents : {}",
        out.stdout
    );
    let v = valeurs(a);
    println!("ce que le modèle contient : {v:?}");
    assert_eq!(
        v,
        vec![1.0, 2.0, 3.0, 4.0, 5.0],
        "l'écriture refusée ne doit rien avoir changé au modèle : {v:?}"
    );
    reinit(a, vec![1.0, 2.0, 3.0, 4.0, 5.0]);
}

#[test]
fn b9_tableau_masque_ecrase_les_cellules_manquantes_par_des_valeurs() {
    // A1 — LE cas qui compte : un masque posé sur des données par ailleurs
    // valides, idiome standard de numpy pour « mesure à écarter »
    // (`np.ma.masked_where(qualite < 3, mesures)`). Corrigée : refusée par
    // `pyst.set_column` (`pyst.InvalidValueError`), plutôt qu'écrite comme des
    // données ordinaires.
    let (a, _b, _s) = ids();
    reinit(a, vec![1.0, 2.0, 3.0, 4.0, 5.0]);
    let out = run("import numpy as np\n\
         h = pyst.resolve('[Book1]Sheet1!A')\n\
         mesures = np.array([10.0, 20.0, 30.0, 40.0, 50.0])\n\
         qualite = np.array([5, 1, 5, 1, 5])\n\
         m = np.ma.masked_where(qualite < 3, mesures)\n\
         print('masque      :', list(m.mask))\n\
         print('vu de Python:', str(m))\n\
         print('moyenne     :', float(m.mean()))\n\
         try:\n\
         \x20   pyst.set_column(h, m)\n\
         \x20   print('refus: non')\n\
         except pyst.InvalidValueError as exc:\n\
         \x20   print('refus: oui', exc)\n");
    assert!(out.error.is_none(), "{:?}", out.error);
    println!("{}", out.stdout);
    assert!(
        out.stdout.contains("refus: oui"),
        "les deux mesures masquées (20 et 40) doivent être refusées, pas \
         écrites comme des données ordinaires : {}",
        out.stdout
    );
    let v = valeurs(a);
    println!("ce que le modèle contient : {v:?}");
    assert_eq!(
        v,
        vec![1.0, 2.0, 3.0, 4.0, 5.0],
        "l'écriture refusée ne doit rien avoir changé au modèle : {v:?}"
    );
    reinit(a, vec![1.0, 2.0, 3.0, 4.0, 5.0]);
}

// ============================================================ concurrence

#[test]
fn b9_course_entre_script_et_interface_sur_la_longueur() {
    // Le script écrit une colonne entière en boucle ; l'interface insère et
    // supprime des lignes en même temps. Le contrat de `pyst.set_column` est
    // « n'allonge ni ne tronque jamais ». On le vérifie en continu.
    //
    // Une écriture dont la colonne a changé depuis la lecture est refusée
    // aussitôt par `ConcurrentWriteError` (`docs/python-et-script.md`,
    // « version lue ») : c'est le contrat, pas une anomalie. Le script la
    // compte comme un refus, comme `InvalidValueError`. Le nombre de refus
    // dépend de l'ordonnancement et n'est pas asserté.
    //
    // L'interface, elle, peut être refusée : tant que le script tient sa macro
    // ouverte, l'acteur refuse les commandes d'un autre client
    // (`ForeignMacro("execute")`, `stokesforge-session`, table de tête). Une
    // suppression acceptée dont la réinsertion est refusée laisse donc une
    // ligne en moins, sans que le script y soit pour rien (constat CI du
    // 2026-10-26 : 999 lignes). L'oracle compte ce que l'interface a réellement
    // réussi : la longueur doit valoir 1000 − suppressions + insertions, exactement.
    //
    // Feuille à part : l'interface insère et supprime des lignes AU NIVEAU
    // FEUILLE. Sur la feuille du banc, elle décalerait et rallongerait les
    // colonnes `A` et `Jetable` que lisent les tests suivants (« expected
    // length 1001 … got 1000 », constat CI du 2026-10-26, tag rc1) : ce test
    // ne tournait pas tant qu'il était `#[ignore]`.
    let (_a, _b, banc_sheet) = ids();
    let h0 = poignee();
    let book = h0
        .snapshot()
        .unwrap()
        .tree
        .parent(banc_sheet)
        .unwrap()
        .unwrap();
    h0.execute(Command::AddWorksheet {
        book,
        name: Some("Course".into()),
    })
    .unwrap();
    let sheet = h0
        .snapshot()
        .unwrap()
        .tree
        .resolve("[Book1]Course")
        .unwrap();
    assert_ne!(sheet, banc_sheet);
    h0.execute(Command::AddColumn {
        sheet,
        index: None,
        name: Some("Course".into()),
        column: Column::from_f64(&vec![0.0; 1000]),
    })
    .unwrap();
    let b = h0
        .snapshot()
        .unwrap()
        .tree
        .resolve("[Book1]Course!Course")
        .unwrap();
    let stop = Arc::new(AtomicBool::new(false));
    let violations = Arc::new(AtomicUsize::new(0));
    let stop2 = Arc::clone(&stop);
    let viol2 = Arc::clone(&violations);
    let h = poignee();
    let secoueur = std::thread::spawn(move || {
        let mut tours = 0usize;
        let (mut retirees, mut remises) = (0i64, 0i64);
        while !stop2.load(Ordering::Relaxed) {
            if h.execute(Command::DeleteRows {
                sheet,
                at: 0,
                count: 1,
            })
            .is_ok()
            {
                retirees += 1;
            }
            if h.execute(Command::InsertRows {
                sheet,
                at: 0,
                count: 1,
            })
            .is_ok()
            {
                remises += 1;
            }
            let n = h
                .snapshot()
                .unwrap()
                .tree
                .node(b)
                .map(|n| n.as_column().unwrap().len())
                .unwrap_or(0);
            if n != 1000 {
                viol2.fetch_add(1, Ordering::Relaxed);
            }
            tours += 1;
        }
        (tours, retirees - remises)
    });

    let out = run("import numpy as np\n\
         h = pyst.resolve('[Book1]Course!Course')\n\
         refus = 0\n\
         for i in range(400):\n\
         \x20   v = pyst.column(h)\n\
         \x20   try:\n\
         \x20       pyst.set_column(h, np.asarray(v, dtype=np.float64) + 1.0)\n\
         \x20   except (pyst.InvalidValueError, pyst.ConcurrentWriteError):\n\
         \x20       refus += 1\n\
         print('refus', refus)\n");
    stop.store(true, Ordering::Relaxed);
    let (tours, retrait_net) = secoueur.join().unwrap();
    println!(
        "{} tours de l'interface, sortie script : {}",
        tours,
        out.stdout.trim()
    );
    assert!(out.error.is_none(), "le script a explosé : {:?}", out.error);
    let n = valeurs(b).len();
    let attendu = 1000 - retrait_net;
    let derive = n as i64 - attendu;
    println!("longueur {n}, attendue {attendu} (interface : {retrait_net} ligne(s) en moins), dérive {derive:+}");
    h0.execute(Command::DeleteColumn { column: b }).unwrap();
    assert_eq!(
        n as i64, attendu,
        "la colonne fait {n} lignes après la course, {attendu} attendues ({derive:+}) : \
         le script (« n'allonge ni ne tronque », longueur exacte contrôlée avant \
         envoi) ne doit pas la faire changer de taille, seule l'interface le peut, \
         de ce qu'elle a réussi"
    );
}

#[test]
fn b9_poignee_vers_une_colonne_supprimee_puis_restauree() {
    let h = poignee();
    // Colonne créée ici, pas dans le banc : les autres tests insèrent et
    // suppriment des lignes AU NIVEAU FEUILLE, ce qui viderait une colonne
    // courte partagée et brouillerait l'observation.
    let (_a, _b, sheet) = ids();
    h.execute(Command::AddColumn {
        sheet,
        index: None,
        name: Some("Jetable2".into()),
        column: Column::from_f64(&[1.0, 2.0, 3.0]),
    })
    .unwrap();
    let jetable = h
        .snapshot()
        .unwrap()
        .tree
        .resolve("[Book1]Sheet1!Jetable2")
        .unwrap();
    let out = run("h = pyst.resolve('[Book1]Sheet1!Jetable2')\nprint('résolue')\n");
    assert!(out.error.is_none(), "{:?}", out.error);

    h.execute(Command::DeleteColumn { column: jetable })
        .unwrap();
    let out = run(
        "try:\n    pyst.column(h)\n    print('LUE MALGRÉ LA SUPPRESSION')\n\
         except pyst.BrokenReferenceError:\n    print('BrokenReferenceError')\n",
    );
    assert!(out.error.is_none(), "{:?}", out.error);
    assert!(
        out.stdout.contains("BrokenReferenceError"),
        "une colonne supprimée reste lisible : {}",
        out.stdout
    );

    h.undo().unwrap();
    let out = run("print([float(x) for x in pyst.column(h)])\n");
    assert!(
        out.error.is_none(),
        "la poignée n'a pas survécu à l'annulation : {:?}",
        out.error
    );
    println!("après annulation : {}", out.stdout.trim());
    assert_eq!(
        out.stdout.trim(),
        "[1.0, 2.0, 3.0]",
        "la colonne restaurée par l'annulation n'a pas retrouvé ses valeurs"
    );
}

// ============================================================ interruption

#[test]
fn b9_interruption_dune_boucle_python_pure() {
    let g = banc().lock().unwrap();
    let engine = &g.engine;
    std::thread::scope(|s| {
        s.spawn(|| {
            // Attend que le script soit vraiment entré, puis interrompt.
            let debut = std::time::Instant::now();
            while !engine.is_running() && debut.elapsed().as_secs() < 10 {
                std::thread::yield_now();
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
            engine.interrupt();
        });
        let out = engine
            .run("i = 0\nwhile True:\n    i += 1\n")
            .expect("accepté");
        assert!(out.interrupted, "le script n'a pas été marqué interrompu");
        assert!(
            out.error
                .as_deref()
                .unwrap_or("")
                .contains("KeyboardInterrupt"),
            "erreur inattendue : {:?}",
            out.error
        );
    });
    // Et le moteur doit rester utilisable ensuite.
    let out = engine.run("print('vivant')").expect("accepté");
    assert!(
        out.error.is_none(),
        "le moteur est cassé après interruption : {:?}",
        out.error
    );
    assert_eq!(out.stdout.trim(), "vivant");
    assert!(
        !out.interrupted,
        "l'interruption a débordé sur le script suivant"
    );
}

#[test]
fn b9_interruption_dune_boucle_qui_avale_les_exceptions() {
    let g = banc().lock().unwrap();
    let engine = &g.engine;
    std::thread::scope(|s| {
        s.spawn(|| {
            let debut = std::time::Instant::now();
            while !engine.is_running() && debut.elapsed().as_secs() < 10 {
                std::thread::yield_now();
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
            engine.interrupt();
        });
        // Boucle qui avale tout MAIS touche à `pyst` : la première moitié du
        // mécanisme (drapeau vérifié par chaque appel) doit la rattraper.
        let out = engine
            .run(
                "while True:\n\
                 \x20   try:\n\
                 \x20       pyst.resolve('[Book1]Sheet1!A')\n\
                 \x20   except pyst.UnknownPathError:\n\
                 \x20       pass\n",
            )
            .expect("accepté");
        assert!(
            out.interrupted,
            "une boucle qui avale les exceptions mais appelle pyst n'a pas été arrêtée"
        );
    });
    let out = engine.run("print('encore vivant')").expect("accepté");
    assert!(out.error.is_none(), "{:?}", out.error);
}

#[test]
fn b9_moteur_refuse_un_second_script_concurrent() {
    let g = banc().lock().unwrap();
    let engine = &g.engine;
    std::thread::scope(|s| {
        let h = s.spawn(|| {
            let debut = std::time::Instant::now();
            while !engine.is_running() && debut.elapsed().as_secs() < 10 {
                std::thread::yield_now();
            }
            let r = engine.run("print('intrus')");
            let verdict = matches!(r, Err(ScriptError::Busy));
            engine.interrupt();
            verdict
        });
        let _ = engine.run("i = 0\nwhile True:\n    i += 1\n");
        assert!(
            h.join().unwrap(),
            "un second script a été accepté pendant le premier"
        );
    });
}

// ================================================================ divers

#[test]
fn b9_vue_conservee_retient_lancien_buffer() {
    // Conséquence documentée : une vue laissée dans une variable retient le
    // buffer d'avant. On vérifie que c'est bien une valeur périmée et jamais
    // un accès invalide, sur un buffer assez gros pour que ce soit visible.
    let (_a, b, _s) = ids();
    reinit(b, vec![0.0; 1000]);
    let out = run("import numpy as np\n\
         h = pyst.resolve('[Book1]Sheet1!B')\n\
         gardee = pyst.column(h)\n\
         somme_avant = float(np.sum(gardee))\n\
         pyst.set_column(h, np.ones(1000))\n\
         print('somme de la vue gardée :', float(np.sum(gardee)))\n\
         print('somme relue           :', float(np.sum(pyst.column(h))))\n\
         print('inchangée :', float(np.sum(gardee)) == somme_avant)\n");
    assert!(out.error.is_none(), "{:?}", out.error);
    println!("{}", out.stdout);
    assert!(out.stdout.contains("inchangée : True"), "{}", out.stdout);
    assert!(
        out.stdout.contains("somme relue           : 1000.0"),
        "{}",
        out.stdout
    );
    reinit(b, vec![0.0; 1000]);
}

#[test]
fn b9_colonne_vide_et_grande_colonne() {
    let h = poignee();
    let (_a, _b, sheet) = ids();
    // Colonne vide : la vue numpy doit être un tableau vide, pas une erreur.
    h.execute(Command::AddColumn {
        sheet,
        index: None,
        name: Some("Vide".into()),
        column: Column::new(ColumnType::Numeric),
    })
    .unwrap();
    let out = run("import numpy as np\n\
         v = pyst.column(pyst.resolve('[Book1]Sheet1!Vide'))\n\
         print('taille', v.size, 'dtype', v.dtype)\n\
         pyst.set_column(pyst.resolve('[Book1]Sheet1!Vide'), np.zeros(0))\n\
         print('écriture vide ok')\n");
    assert!(out.error.is_none(), "{:?}", out.error);
    println!("{}", out.stdout.trim());
    assert!(out.stdout.contains("taille 0"), "{}", out.stdout);
}

#[test]
fn b9_script_qui_garde_des_centaines_de_vues() {
    let (_a, b, _s) = ids();
    reinit(b, vec![0.0; 1000]);
    let out = run("import numpy as np\n\
         h = pyst.resolve('[Book1]Sheet1!B')\n\
         vues = []\n\
         for i in range(300):\n\
         \x20   vues.append(pyst.column(h))\n\
         \x20   pyst.set_column(h, np.full(1000, float(i)))\n\
         print('vues gardées :', len(vues))\n\
         print('première :', float(vues[0][0]), 'dernière :', float(vues[-1][0]))\n\
         vues.clear()\n\
         print('relâchées')\n");
    assert!(out.error.is_none(), "{:?}", out.error);
    println!("{}", out.stdout);
    assert!(out.stdout.contains("relâchées"));
}

#[test]
fn b9_erreur_de_lhote_ne_laisse_pas_le_moteur_dans_un_etat_bancal() {
    for _ in 0..50 {
        let out = run(
            "try:\n    pyst.resolve('[Inexistant]X!Y')\nexcept pyst.UnknownPathError:\n    pass\n",
        );
        assert!(out.error.is_none(), "{:?}", out.error);
    }
    let out = run("print('toujours là')");
    assert_eq!(out.stdout.trim(), "toujours là");
}

#[test]
fn b9_interruption_avalee_annule_aussi_le_graphe_et_les_valeurs() {
    // Banc propre : ne pas tenir le verrou du banc partagé en attendant le moteur.
    let mut project = Project::new("Arrêt").unwrap();
    project.add_workbook(project.root(), Some("Book1")).unwrap();
    let sheet = project.resolve_path("[Book1]Sheet1").unwrap();
    project
        .add_column(sheet, None, Some("A"), Column::from_f64(&[1., 2.]))
        .unwrap();
    let actor = Session::spawn(project, None).unwrap();
    let handle = actor.handle();
    let before = handle.snapshot().unwrap();
    let engine =
        Arc::new(ScriptEngine::spawn(handle.clone(), EmbeddedPython::locate().unwrap()).unwrap());
    let worker = Arc::clone(&engine);
    let job = std::thread::spawn(move || {
        worker
            .run(
                r#"
a = pyst.resolve('[Book1]Sheet1!A')
try:
    pyst.set_values(a, [9.,10.,11.])
    g = pyst.plot(a, name='À annuler')
    pyst.add_shape(g, 'rectangle', 0, 0, 1, 1)
    while True: pass
except KeyboardInterrupt:
    print('interruption avalée')
"#,
            )
            .unwrap()
    });
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        let snapshot = handle.snapshot().unwrap();
        if snapshot.tree.resolve("[À annuler]").ok().is_some_and(|id| {
            matches!(snapshot.tree.node(id).unwrap().kind(), stokesforge_core::NodeKind::Graph { spec, .. } if !spec.panels[0].shapes.is_empty())
        })        {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "graphe et objet non créés"
        );
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    engine.interrupt();
    let outcome = job.join().unwrap();
    assert!(outcome.interrupted);
    assert!(outcome.error.is_none(), "{:?}", outcome.error);
    assert!(outcome.stdout.contains("interruption avalée"));
    let after = handle.snapshot().unwrap();
    assert_eq!(*after.tree, *before.tree);
    assert_eq!(after.history, before.history);
    let followup = engine
        .run("assert pyst.values(pyst.resolve('[Book1]Sheet1!A')) == (1.,2.)")
        .unwrap();
    assert!(followup.error.is_none());
}

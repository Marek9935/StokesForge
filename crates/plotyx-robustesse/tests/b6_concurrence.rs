//! Batterie 6 — accès concurrents à l'état partagé (l'acteur de session).

use plotyx_commands::Command;
use plotyx_core::{Column, ColumnType, NodeId, Project, ValueBlock};
use plotyx_session::{Session, SessionError, SessionHandle};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Barrier};

fn session(colonnes: usize, lignes: usize) -> (Session, NodeId, Vec<NodeId>) {
    let mut p = Project::new("C").unwrap();
    let root = p.root();
    p.add_workbook(root, Some("Book1")).unwrap();
    let sheet = p.resolve_path("[Book1]Sheet1").unwrap();
    let mut cols = Vec::new();
    for i in 0..colonnes {
        let valeurs: Vec<f64> = (0..lignes).map(|r| (i * 1000 + r) as f64).collect();
        cols.push(
            p.add_column(
                sheet,
                None,
                Some(&format!("C{i}")),
                Column::from_f64(&valeurs),
            )
            .unwrap(),
        );
    }
    let sheet_id = sheet;
    (Session::spawn(p, None).unwrap(), sheet_id, cols)
}

fn longueur(h: &SessionHandle, col: NodeId) -> usize {
    h.snapshot()
        .unwrap()
        .tree
        .node(col)
        .unwrap()
        .as_column()
        .unwrap()
        .len()
}

fn valeurs(h: &SessionHandle, col: NodeId) -> Vec<f64> {
    match h
        .snapshot()
        .unwrap()
        .tree
        .node(col)
        .unwrap()
        .as_column()
        .unwrap()
        .values()
    {
        plotyx_core::ColumnValues::Float(v) => v.as_ref().clone(),
        _ => panic!("colonne texte"),
    }
}

#[test]
fn b6_ecritures_concurrentes_restent_serialisees() {
    let (s, _sheet, cols) = session(4, 50);
    let barriere = Arc::new(Barrier::new(8));
    let mut fils = Vec::new();
    for t in 0..8usize {
        let h = s.handle();
        let b = Arc::clone(&barriere);
        let cols = cols.clone();
        fils.push(std::thread::spawn(move || {
            b.wait();
            let mut ok = 0usize;
            for i in 0..200u64 {
                let col = cols[(t + i as usize) % cols.len()];
                let v = (t as f64) * 1e6 + i as f64;
                if h.execute(Command::SetCells {
                    column: col,
                    start: (i % 50) as usize,
                    values: ValueBlock::Float(vec![v]),
                })
                .is_ok()
                {
                    ok += 1;
                }
            }
            ok
        }));
    }
    let total: usize = fils.into_iter().map(|f| f.join().unwrap()).sum();
    assert_eq!(total, 8 * 200, "des commandes ont été perdues");
    let h = s.handle();
    let snap = h.snapshot().unwrap();
    assert_eq!(
        snap.revision as usize, total,
        "la révision ne compte pas les mutations"
    );
    for c in &cols {
        assert_eq!(longueur(&h, *c), 50, "une colonne a changé de longueur");
    }
    s.shutdown().unwrap();
}

#[test]
fn b6_instantane_est_fige_malgre_les_ecritures() {
    let (s, _sheet, cols) = session(1, 1000);
    let h = s.handle();
    let snap = h.snapshot().unwrap();
    let avant: Vec<f64> = match snap
        .tree
        .node(cols[0])
        .unwrap()
        .as_column()
        .unwrap()
        .values()
    {
        plotyx_core::ColumnValues::Float(v) => v.as_ref().clone(),
        _ => panic!(),
    };
    for i in 0..500u64 {
        h.execute(Command::SetCells {
            column: cols[0],
            start: i as usize,
            values: ValueBlock::Float(vec![-1.0]),
        })
        .unwrap();
    }
    let apres: Vec<f64> = match snap
        .tree
        .node(cols[0])
        .unwrap()
        .as_column()
        .unwrap()
        .values()
    {
        plotyx_core::ColumnValues::Float(v) => v.as_ref().clone(),
        _ => panic!(),
    };
    assert_eq!(
        avant, apres,
        "l'instantané a bougé sous les pieds de son détenteur"
    );
    assert!(snap.revision < h.snapshot().unwrap().revision);
    s.shutdown().unwrap();
}

#[test]
#[ignore = "reproduction A4 — l'avortement d'une macro efface le travail validé d'un autre client, sans notification"]
fn b6_macro_dun_client_avale_le_travail_dun_autre() {
    // Limite documentée en tête de `plotyx-session` : une macro est globale à
    // la session. On en mesure ici la conséquence exacte.
    let (s, _sheet, cols) = session(2, 3);
    let ui = s.handle();
    let script = s.handle();

    ui.begin_macro("import").unwrap();
    script
        .execute(Command::SetCells {
            column: cols[1],
            start: 0,
            values: ValueBlock::Float(vec![42.0, 43.0, 44.0]),
        })
        .unwrap();
    assert_eq!(valeurs(&script, cols[1])[0], 42.0);

    ui.abort_macro().unwrap();

    let apres = valeurs(&script, cols[1])[0];
    assert_eq!(
        apres, 42.0,
        "l'avortement de la macro d'un client a effacé le travail d'un autre \
         client (valeur revenue à {apres}) sans que ce dernier en soit informé"
    );
    s.shutdown().unwrap();
}

#[test]
#[ignore = "reproduction A2 — le contrôle de longueur du pont porte sur un instantané périmé"]
fn b6_toctou_entre_instantane_et_commande() {
    // Reproduit exactement la séquence de `plotyx_script::bridge::set_column` :
    //   1. instantané   2. contrôle de longueur   3. envoi de la commande
    // Un autre client peut se glisser entre 2 et 3.
    let (s, sheet, cols) = session(1, 10);
    let client = s.handle();
    let autre = s.handle();
    let col = cols[0];

    let snap = client.snapshot().unwrap();
    let longueur_vue = snap.tree.node(col).unwrap().as_column().unwrap().len();
    assert_eq!(longueur_vue, 10);
    let nouvelles: Vec<f64> = (0..longueur_vue).map(|i| 100.0 + i as f64).collect();

    autre
        .execute(Command::DeleteRows {
            sheet,
            at: 5,
            count: 5,
        })
        .unwrap();
    assert_eq!(longueur(&autre, col), 5);

    client
        .execute(Command::SetCells {
            column: col,
            start: 0,
            values: ValueBlock::Float(nouvelles),
        })
        .unwrap();

    let finale = longueur(&client, col);
    assert_eq!(
        finale, 5,
        "le contrôle « longueur exacte » du pont a été contourné : la colonne \
         est passée de 5 à {finale} lignes alors que l'API de script promet de \
         ne jamais allonger ni tronquer une colonne"
    );
    s.shutdown().unwrap();
}

#[test]
fn b6_acteur_arrete_rend_disconnected_et_pas_pire() {
    let (s, _sheet, cols) = session(1, 3);
    let h = s.handle();
    let projet = s.shutdown().unwrap();
    assert_eq!(projet.tree().len(), 4);
    assert!(matches!(h.snapshot(), Err(SessionError::Disconnected)));
    assert!(matches!(h.undo(), Err(SessionError::Disconnected)));
    assert!(matches!(h.redo(), Err(SessionError::Disconnected)));
    assert!(matches!(
        h.begin_macro("x"),
        Err(SessionError::Disconnected)
    ));
    assert!(matches!(h.end_macro(), Err(SessionError::Disconnected)));
    assert!(matches!(
        h.execute(Command::SetCells {
            column: cols[0],
            start: 0,
            values: ValueBlock::Float(vec![1.0])
        }),
        Err(SessionError::Disconnected)
    ));
}

#[test]
fn b6_undo_concurrent_ne_double_compte_pas() {
    let (s, _sheet, cols) = session(1, 10);
    let h = s.handle();
    for i in 0..100u64 {
        h.execute(Command::SetCells {
            column: cols[0],
            start: 0,
            values: ValueBlock::Float(vec![i as f64]),
        })
        .unwrap();
    }
    let annules = Arc::new(AtomicUsize::new(0));
    let barriere = Arc::new(Barrier::new(4));
    let mut fils = Vec::new();
    for _ in 0..4 {
        let h = s.handle();
        let b = Arc::clone(&barriere);
        let n = Arc::clone(&annules);
        fils.push(std::thread::spawn(move || {
            b.wait();
            while let Ok(Some(_)) = h.undo() {
                n.fetch_add(1, Ordering::Relaxed);
            }
        }));
    }
    for f in fils {
        f.join().unwrap();
    }
    assert_eq!(
        annules.load(Ordering::Relaxed),
        100,
        "quatre clients annulant en même temps n'ont pas annulé exactement les 100 crans"
    );
    assert!(!h.snapshot().unwrap().history.can_undo);
    s.shutdown().unwrap();
}

#[test]
#[ignore = "reproduction A3 — une poignée d'avant un rechargement désigne un nœud du nouveau projet, et l'écriture est acceptée"]
fn b6_poignee_survit_a_un_load_et_designe_un_autre_objet() {
    // Deux projets DIFFÉRENTS de même forme : les identifiants de nœuds sont
    // les mêmes (l'allocateur repart du fichier), les données non.
    let dir = tempfile::TempDir::new().unwrap();
    let chemin = dir.path().join("autre.plx");
    {
        let mut p = Project::new("AUTRE").unwrap();
        let root = p.root();
        p.add_workbook(root, Some("Book1")).unwrap();
        let sheet = p.resolve_path("[Book1]Sheet1").unwrap();
        // Même position, même identifiant, contenu tout autre.
        p.add_column(
            sheet,
            None,
            Some("C0"),
            Column::from_f64(&[-7.0, -8.0, -9.0, -10.0, -11.0]),
        )
        .unwrap();
        p.add_column(sheet, None, Some("C1"), Column::from_f64(&[0.0; 5]))
            .unwrap();
        plotyx_persistence::save(&p, &chemin).unwrap();
    }

    let (s, _sheet, cols) = session(2, 5);
    let client = s.handle();
    let col = cols[0]; // la poignée que tiendrait un script : NodeId brut

    assert_eq!(valeurs(&client, col)[0], 0.0, "mise en place");

    // Un autre client (l'UI, un menu Fichier > Ouvrir) recharge un projet.
    client.load(&chemin).unwrap();

    // La poignée d'avant n'est ni refusée, ni signalée : elle désigne
    // désormais une colonne d'un autre projet.
    let apres = valeurs(&client, col);
    println!("la poignée {col} désigne maintenant : {apres:?}");
    let r = client.execute(Command::SetCells {
        column: col,
        start: 0,
        values: ValueBlock::Float(vec![999.0]),
    });
    assert!(
        r.is_err(),
        "une écriture par une poignée d'avant le rechargement a été ACCEPTÉE : \
         elle a écrit dans la colonne « {} » du projet fraîchement chargé",
        client.snapshot().unwrap().tree.node(col).unwrap().name()
    );
    s.shutdown().unwrap();
}

#[test]
fn b6_ecriture_sur_un_noeud_supprime_par_un_autre_client() {
    let (s, _sheet, cols) = session(2, 5);
    let a = s.handle();
    let b = s.handle();
    a.execute(Command::DeleteColumn { column: cols[0] })
        .unwrap();
    let r = b.execute(Command::SetCells {
        column: cols[0],
        start: 0,
        values: ValueBlock::Float(vec![1.0]),
    });
    assert!(r.is_err(), "écriture acceptée sur une colonne supprimée");
    a.undo().unwrap();
    let r2 = b.execute(Command::SetCells {
        column: cols[0],
        start: 0,
        values: ValueBlock::Float(vec![1.0]),
    });
    assert!(
        r2.is_ok(),
        "la poignée n'a pas survécu à suppression + annulation : {r2:?}"
    );
    s.shutdown().unwrap();
}

#[test]
fn b6_type_incompatible_entre_deux_clients() {
    let (s, _sheet, cols) = session(1, 5);
    let a = s.handle();
    let b = s.handle();
    a.execute(Command::SetColumnType {
        column: cols[0],
        column_type: ColumnType::Text,
    })
    .unwrap();
    let r = b.execute(Command::SetCells {
        column: cols[0],
        start: 0,
        values: ValueBlock::Float(vec![1.0]),
    });
    assert!(
        r.is_err(),
        "un bloc numérique a été écrit dans une colonne devenue texte"
    );
    s.shutdown().unwrap();
}

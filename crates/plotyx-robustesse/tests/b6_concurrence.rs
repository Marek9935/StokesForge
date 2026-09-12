//! Batterie 6 — accès concurrents à l'état partagé (l'acteur de session).

use plotyx_commands::{Command, CommandError};
use plotyx_core::{Column, ColumnType, CoreError, NodeId, Project, ValueBlock};
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
                    expected_version: None,
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
            expected_version: None,
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
fn b6_macro_dun_client_avale_le_travail_dun_autre() {
    // Limite documentée à l'origine en tête de `plotyx-session` : une macro
    // était globale à la session. On en mesurait ici la conséquence exacte.
    //
    // A4 (`guide-de-correction-robustesse.md` §F6) — décidé : chaque
    // `SessionHandle` issu d'un appel indépendant à `Session::handle()` porte
    // une identité (`ClientId`) distincte, et l'acteur refuse
    // (`CommandError::ForeignMacro`) qu'un `Execute` d'un émetteur autre que
    // le propriétaire de la macro courante s'y absorbe. L'assertion change
    // donc de forme : à l'origine, l'écriture de `script` était acceptée
    // (absorbée dans la macro d'`ui`) puis silencieusement défaite par
    // `abort_macro` — c'était le bug. Avec le correctif, cette même écriture
    // est refusée **au moment de l'exécuter**, avant même d'entrer dans le
    // modèle : il n'y a donc plus rien à défaire. On vérifie les deux moitiés
    // de la garantie : le refus immédiat pendant la macro d'un autre, puis
    // que `script` retrouve un fonctionnement normal une fois cette macro
    // refermée — son travail n'a jamais dépendu du sort de celle d'`ui`.
    let (s, _sheet, cols) = session(2, 3);
    let ui = s.handle();
    let script = s.handle();
    let valeur_initiale = valeurs(&script, cols[1])[0];

    ui.begin_macro("import").unwrap();
    let refus = script.execute(Command::SetCells {
        column: cols[1],
        start: 0,
        values: ValueBlock::Float(vec![42.0, 43.0, 44.0]),
        expected_version: None,
    });
    assert!(
        matches!(
            refus,
            Err(SessionError::Command(CommandError::ForeignMacro("execute")))
        ),
        "l'écriture de `script` aurait dû être refusée pendant que la macro d'`ui` est \
         ouverte, pas {refus:?}"
    );
    assert_eq!(
        valeurs(&script, cols[1])[0],
        valeur_initiale,
        "un `Execute` refusé n'aurait jamais dû toucher au modèle"
    );

    ui.abort_macro().unwrap();

    // `script` n'a jamais été absorbé par la macro d'`ui` : une fois celle-ci
    // refermée (ici avortée), il retente et réussit normalement.
    script
        .execute(Command::SetCells {
            column: cols[1],
            start: 0,
            values: ValueBlock::Float(vec![42.0, 43.0, 44.0]),
            expected_version: None,
        })
        .unwrap();

    let apres = valeurs(&script, cols[1])[0];
    assert_eq!(
        apres, 42.0,
        "le travail de `script`, appliqué après la fermeture de la macro d'`ui`, doit rester \
         à 42.0 — il n'a jamais fait partie de cette macro et ne doit donc jamais être défait \
         par son abandon (valeur observée : {apres})"
    );
    s.shutdown().unwrap();
}

#[test]
fn b6_toctou_entre_instantane_et_commande() {
    // Reproduit exactement la séquence de `plotyx_script::bridge::set_column` :
    //   1. instantané   2. lecture de `Column::version`   3. envoi de la commande
    // Un autre client peut se glisser entre 2 et 3.
    //
    // A2 (`guide-de-correction-robustesse.md` §F6) — décidé : comparer
    // `Column::version` (qui change à TOUTE modification, pas seulement un
    // changement de longueur), vérifié par l'acteur au moment d'APPLIQUER la
    // commande, jamais avant. Cette reproduction portait à l'origine un
    // contrôle de **longueur** fait par le client avant l'envoi (le bug
    // qu'A2 corrige) ; son assertion attendait alors que la colonne change
    // de longueur pour prouver que ce contrôle avait été contourné. Le
    // correctif change ce que le pont vérifie — plus la longueur vue avant
    // l'envoi, mais `expected_version` vérifié par l'acteur à l'application
    // — donc l'assertion change de forme : on vérifie maintenant que
    // l'écriture bâtie sur un `expected_version` périmé est refusée avec
    // l'erreur dédiée, et que le modèle n'a pas bougé sous ce refus.
    let (s, sheet, cols) = session(1, 10);
    let client = s.handle();
    let autre = s.handle();
    let col = cols[0];

    let snap = client.snapshot().unwrap();
    let colonne_vue = snap.tree.node(col).unwrap().as_column().unwrap();
    assert_eq!(colonne_vue.len(), 10);
    let version_vue = colonne_vue.version();
    let nouvelles: Vec<f64> = (0..10).map(|i| 100.0 + i as f64).collect();

    autre
        .execute(Command::DeleteRows {
            sheet,
            at: 5,
            count: 5,
        })
        .unwrap();
    assert_eq!(longueur(&autre, col), 5);

    let r = client.execute(Command::SetCells {
        column: col,
        start: 0,
        values: ValueBlock::Float(nouvelles),
        expected_version: Some(version_vue),
    });
    assert!(
        matches!(
            r,
            Err(SessionError::Command(CommandError::Core(
                CoreError::ConcurrentModification { .. }
            )))
        ),
        "l'écriture bâtie sur la version {version_vue} (celle de l'instantané) aurait dû être \
         refusée pour cause de version concurrente, pas {r:?}"
    );

    let finale = longueur(&client, col);
    assert_eq!(
        finale, 5,
        "le refus fondé sur `expected_version` a tout de même laissé la colonne changer de \
         longueur : elle est passée de 5 à {finale} lignes"
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
            values: ValueBlock::Float(vec![1.0]),
            expected_version: None,
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
            expected_version: None,
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

/// Poignée simulée : exactement ce que mémorise
/// `plotyx_script::surface::Handle` (`NodeId` + `epoch`), et exactement la
/// vérification que `plotyx_script::bridge::{float_buffer, set_column}`
/// appliquent **avant** de chercher `id` dans l'arbre courant (A3,
/// `guide-de-correction-robustesse.md` §F2/F6). Ce module est privé au
/// crate `plotyx-script` et derrière la feature `python` — inatteignable
/// depuis `plotyx-robustesse` sans elle — donc cette reproduction rejoue la
/// même logique à la main, sur les primitives publiques de
/// `plotyx-session` (`Snapshot::epoch`, ajouté par ce correctif), pour
/// rester exécutable sans l'interpréteur embarqué. C'est fidèle : la
/// logique réelle du pont ne fait rien de plus que ceci.
struct PoigneeSimulee {
    id: NodeId,
    epoch: u64,
}

impl PoigneeSimulee {
    fn resoudre(h: &SessionHandle, id: NodeId) -> Self {
        let epoch = h.snapshot().unwrap().epoch;
        PoigneeSimulee { id, epoch }
    }

    /// Écrit par la commande, **sauf** si l'époque mémorisée ne coïncide
    /// plus avec celle de l'instantané courant — refus **avant** même de
    /// consulter l'arbre, pour la raison précise que cette reproduction
    /// existe : le `NodeId` peut, par coïncidence, désigner un autre objet
    /// dans le nouveau projet.
    fn set_column(&self, h: &SessionHandle, values: Vec<f64>) -> Result<(), &'static str> {
        let snapshot = h.snapshot().unwrap();
        if snapshot.epoch != self.epoch {
            return Err(
                "époque périmée : un chargement a eu lieu depuis que cette poignée a été obtenue",
            );
        }
        h.execute(Command::SetCells {
            column: self.id,
            start: 0,
            values: ValueBlock::Float(values),
            expected_version: None,
        })
        .map(|_| ())
        .map_err(|_| "refusé par l'acteur")
    }
}

#[test]
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
    let col = cols[0];
    // La poignée que tiendrait un script : `resolve()` la produit avant le
    // rechargement, avec l'époque en vigueur à cet instant.
    let poignee = PoigneeSimulee::resoudre(&client, col);

    assert_eq!(valeurs(&client, col)[0], 0.0, "mise en place");

    // Un autre client (l'UI, un menu Fichier > Ouvrir) recharge un projet.
    client.load(&chemin).unwrap();

    // Le modèle, lui, ne refuse ni ne signale rien : le `NodeId` désigne
    // désormais une colonne d'un autre projet — c'est précisément pourquoi
    // la vérification doit être faite par la poignée, pas espérée du
    // modèle (F6 : « toute hypothèse doit voyager dans la requête »,
    // et ici c'est l'ÉMETTEUR qui porte l'hypothèse, avant même d'émettre).
    let apres = valeurs(&client, col);
    println!("le NodeId désigne maintenant : {apres:?}");

    let r = poignee.set_column(&client, vec![999.0]);
    assert!(
        r.is_err(),
        "une écriture par une poignée d'avant le rechargement a été ACCEPTÉE : \
         elle aurait écrit dans la colonne « {} » du projet fraîchement chargé",
        client.snapshot().unwrap().tree.node(col).unwrap().name()
    );
    // Le refus doit venir de l'époque, **avant** que la commande n'atteigne
    // l'acteur — pas d'un refus accidentel du modèle (qui accepterait très
    // bien cette écriture, voir `apres` ci-dessus).
    assert_eq!(
        r,
        Err("époque périmée : un chargement a eu lieu depuis que cette poignée a été obtenue")
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
        expected_version: None,
    });
    assert!(r.is_err(), "écriture acceptée sur une colonne supprimée");
    a.undo().unwrap();
    let r2 = b.execute(Command::SetCells {
        column: cols[0],
        start: 0,
        values: ValueBlock::Float(vec![1.0]),
        expected_version: None,
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
        expected_version: None,
    });
    assert!(
        r.is_err(),
        "un bloc numérique a été écrit dans une colonne devenue texte"
    );
    s.shutdown().unwrap();
}

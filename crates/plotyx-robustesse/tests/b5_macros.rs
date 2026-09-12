//! Batterie 5 — macros imbriquées, avortement, et la pile de rétablissement.
//! C'est la partie de l'historique que `undo_random.rs` du dépôt ne tire pas.

use plotyx_commands::{Command, History};
use plotyx_core::{Column, ColumnType, NodeId, Project, Tree, ValueBlock};
use std::sync::Arc;

fn projet() -> (Project, NodeId, Vec<NodeId>) {
    let mut p = Project::new("M").unwrap();
    let root = p.root();
    p.add_workbook(root, Some("Book1")).unwrap();
    let sheet = p.resolve_path("[Book1]Sheet1").unwrap();
    let mut cols = Vec::new();
    for (i, nom) in ["A", "B", "C"].iter().enumerate() {
        cols.push(
            p.add_column(
                sheet,
                None,
                Some(nom),
                Column::from_f64(&[i as f64, i as f64 + 1.0, i as f64 + 2.0]),
            )
            .unwrap(),
        );
    }
    (p, sheet, cols)
}

fn identique(a: &Arc<Tree>, b: &Arc<Tree>) -> bool {
    a.as_ref() == b.as_ref()
}

#[test]
fn b5_macro_avortee_ne_laisse_aucune_trace() {
    let (mut p, sheet, cols) = projet();
    let mut h = History::new();
    // Un peu d'historique préalable, et un cran annulé pour peupler le redo.
    h.execute(
        &mut p,
        Command::SetCells {
            column: cols[0],
            start: 0,
            values: ValueBlock::Float(vec![9.0]),
            expected_version: None,
        },
    )
    .unwrap();
    h.execute(
        &mut p,
        Command::Rename {
            id: cols[1],
            name: "BB".into(),
        },
    )
    .unwrap();
    h.undo(&mut p).unwrap();
    let avant = Arc::clone(p.tree());
    let (undo_avant, redo_avant) = (h.len(), h.redo_len());
    assert_eq!(
        redo_avant, 1,
        "mise en place : un cran doit être rétablissable"
    );

    h.begin_macro("essai").unwrap();
    h.execute(
        &mut p,
        Command::SetCells {
            column: cols[0],
            start: 0,
            values: ValueBlock::Float(vec![1.0, 2.0]),
            expected_version: None,
        },
    )
    .unwrap();
    h.execute(
        &mut p,
        Command::AddColumn {
            sheet,
            index: None,
            name: Some("Z".into()),
            column: Column::new(ColumnType::Numeric),
        },
    )
    .unwrap();
    h.execute(&mut p, Command::DeleteColumn { column: cols[2] })
        .unwrap();
    h.abort_macro(&mut p).unwrap();

    assert!(
        identique(&avant, p.tree()),
        "l'arbre diffère après avortement"
    );
    assert_eq!(h.len(), undo_avant, "la pile d'annulation a changé");
    assert_eq!(
        h.redo_len(),
        redo_avant,
        "la pile de rétablissement n'a pas été restaurée"
    );
    // Et le rétablissement sauvé doit toujours marcher.
    assert!(h.redo(&mut p).unwrap(), "le cran rétablissable est perdu");
}

#[test]
fn b5_macro_imbriquee_avortee_ne_touche_pas_la_parente() {
    let (mut p, sheet, cols) = projet();
    let mut h = History::new();
    h.begin_macro("externe").unwrap();
    h.execute(
        &mut p,
        Command::Rename {
            id: cols[0],
            name: "AA".into(),
        },
    )
    .unwrap();
    let apres_externe = Arc::clone(p.tree());

    h.begin_macro("interne").unwrap();
    h.execute(
        &mut p,
        Command::AddColumn {
            sheet,
            index: None,
            name: Some("Y".into()),
            column: Column::new(ColumnType::Numeric),
        },
    )
    .unwrap();
    h.execute(
        &mut p,
        Command::SetCells {
            column: cols[1],
            start: 0,
            values: ValueBlock::Float(vec![5.0]),
            expected_version: None,
        },
    )
    .unwrap();
    h.abort_macro(&mut p).unwrap();

    assert!(
        identique(&apres_externe, p.tree()),
        "l'avortement de la macro interne a défait (ou laissé) autre chose que ses propres pas"
    );
    assert_eq!(h.macro_depth(), 1, "la macro externe doit rester ouverte");
    h.end_macro(&mut p).unwrap();
    assert_eq!(h.len(), 1, "la macro externe doit faire une entrée");
    assert!(h.undo(&mut p).unwrap());
    assert_eq!(
        p.tree().node(cols[0]).unwrap().name(),
        "A",
        "annuler la macro externe n'a pas rendu son nom à la colonne"
    );
}

#[test]
fn b5_macro_vide_ne_laisse_rien_et_rend_le_redo() {
    let (mut p, _sheet, cols) = projet();
    let mut h = History::new();
    h.execute(
        &mut p,
        Command::Rename {
            id: cols[0],
            name: "AA".into(),
        },
    )
    .unwrap();
    h.undo(&mut p).unwrap();
    assert_eq!(h.redo_len(), 1);

    h.begin_macro("vide").unwrap();
    h.end_macro(&mut p).unwrap();
    assert_eq!(h.len(), 0);
    assert_eq!(h.redo_len(), 1, "une macro vide a mangé le rétablissement");
}

#[test]
fn b5_macro_avec_commande_refusee_reste_coherente() {
    let (mut p, sheet, cols) = projet();
    let mut h = History::new();
    let avant = Arc::clone(p.tree());
    h.begin_macro("mixte").unwrap();
    h.execute(
        &mut p,
        Command::Rename {
            id: cols[0],
            name: "AA".into(),
        },
    )
    .unwrap();
    // Refusée : « B » est déjà pris dans la feuille.
    let refus = h.execute(
        &mut p,
        Command::Rename {
            id: cols[0],
            name: "B".into(),
        },
    );
    assert!(
        refus.is_err(),
        "un renommage vers un nom pris doit être refusé"
    );
    h.execute(
        &mut p,
        Command::SetCells {
            column: cols[2],
            start: 1,
            values: ValueBlock::Float(vec![8.0, 9.0, 10.0]),
            expected_version: None,
        },
    )
    .unwrap();
    // Et une commande sur une feuille qui n'existe pas.
    let refus2 = h.execute(
        &mut p,
        Command::InsertRows {
            sheet: NodeId::from_raw(4242),
            at: 0,
            count: 1,
        },
    );
    assert!(refus2.is_err());
    h.end_macro(&mut p).unwrap();

    assert_eq!(h.len(), 1);
    assert!(h.undo(&mut p).unwrap());
    assert!(
        identique(&avant, p.tree()),
        "annuler la macro n'a pas ramené l'arbre initial malgré des commandes refusées en son sein"
    );
    let _ = sheet;
}

#[test]
fn b5_macro_profondement_imbriquee() {
    let (mut p, _sheet, cols) = projet();
    let mut h = History::new();
    let avant = Arc::clone(p.tree());
    const N: usize = 64;
    for i in 0..N {
        h.begin_macro(&format!("n{i}")).unwrap();
        h.execute(
            &mut p,
            Command::SetCells {
                column: cols[i % 3],
                start: 0,
                values: ValueBlock::Float(vec![i as f64]),
                expected_version: None,
            },
        )
        .unwrap();
    }
    for _ in 0..N {
        h.end_macro(&mut p).unwrap();
    }
    assert_eq!(h.macro_depth(), 0);
    assert_eq!(
        h.len(),
        1,
        "les {N} macros imbriquées doivent faire une seule entrée"
    );
    assert!(h.undo(&mut p).unwrap());
    assert!(
        identique(&avant, p.tree()),
        "l'imbrication profonde ne s'annule pas complètement"
    );
    assert!(h.redo(&mut p).unwrap(), "et ne se rétablit pas");
}

#[test]
fn b5_undo_refuse_en_macro_ne_casse_rien() {
    let (mut p, _sheet, cols) = projet();
    let mut h = History::new();
    h.execute(
        &mut p,
        Command::Rename {
            id: cols[0],
            name: "AA".into(),
        },
    )
    .unwrap();
    h.begin_macro("m").unwrap();
    h.execute(
        &mut p,
        Command::Rename {
            id: cols[1],
            name: "BB".into(),
        },
    )
    .unwrap();
    let pendant = Arc::clone(p.tree());
    assert!(h.undo(&mut p).is_err(), "undo doit être refusé en macro");
    assert!(h.redo(&mut p).is_err(), "redo doit être refusé en macro");
    assert!(h.clear().is_err(), "clear doit être refusé en macro");
    assert!(
        identique(&pendant, p.tree()),
        "une opération refusée pendant une macro a modifié l'arbre"
    );
    h.end_macro(&mut p).unwrap();
}

#[test]
fn b5_macro_puis_annulation_totale_ramene_larbre_initial() {
    // Séquence longue mêlant macros, structure et données, puis undo complet.
    let (mut p, sheet, cols) = projet();
    let mut h = History::new();
    let initial = Arc::clone(p.tree());

    for tour in 0..20u64 {
        if tour % 3 == 0 {
            h.begin_macro(&format!("t{tour}")).unwrap();
        }
        // Le bloc doit suivre le type courant de la colonne visée : une
        // commande refusée pour cause de type est un refus légitime du cœur,
        // pas ce que ce test cherche.
        let cible = cols[(tour % 3) as usize];
        let bloc = match p
            .tree()
            .node(cible)
            .unwrap()
            .as_column()
            .unwrap()
            .column_type()
        {
            ColumnType::Text => ValueBlock::Text(vec![Some(format!("t{tour}")), None]),
            _ => ValueBlock::Float(vec![tour as f64, f64::NAN]),
        };
        h.execute(
            &mut p,
            Command::SetCells {
                column: cible,
                start: (tour % 4) as usize,
                values: bloc,
                expected_version: None,
            },
        )
        .unwrap();
        if tour % 5 == 0 {
            let nom = format!("N{tour}");
            h.execute(
                &mut p,
                Command::AddColumn {
                    sheet,
                    index: None,
                    name: Some(nom),
                    column: Column::from_f64(&[tour as f64]),
                },
            )
            .unwrap();
        }
        if tour % 7 == 0 {
            h.execute(
                &mut p,
                Command::SetColumnType {
                    column: cols[1],
                    column_type: if tour % 2 == 0 {
                        ColumnType::Text
                    } else {
                        ColumnType::Numeric
                    },
                },
            )
            .unwrap();
        }
        if tour % 3 == 2 && h.in_macro() {
            h.end_macro(&mut p).unwrap();
        }
    }
    while h.in_macro() {
        h.end_macro(&mut p).unwrap();
    }
    let mut crans = 0;
    while h.undo(&mut p).unwrap() {
        crans += 1;
        assert!(crans < 1000, "boucle d'annulation sans fin");
    }
    assert!(
        identique(&initial, p.tree()),
        "après annulation complète, l'arbre n'est pas l'arbre initial"
    );
    while h.redo(&mut p).unwrap() {}
    let mut crans2 = 0;
    while h.undo(&mut p).unwrap() {
        crans2 += 1;
    }
    assert_eq!(crans, crans2, "le nombre de crans diffère au second tour");
    assert!(
        identique(&initial, p.tree()),
        "second aller-retour infidèle"
    );
}

#[test]
fn b5_limite_en_octets_ne_fait_pas_disparaitre_le_dernier_cran() {
    let (mut p, _sheet, cols) = projet();
    // Limite en octets minuscule : chaque entrée la dépasse.
    let mut h = History::with_limits(0, 1);
    for i in 0..10u64 {
        h.execute(
            &mut p,
            Command::SetCells {
                column: cols[0],
                start: 0,
                values: ValueBlock::Float(vec![i as f64; 100]),
                expected_version: None,
            },
        )
        .unwrap();
    }
    assert!(
        h.can_undo(),
        "la limite en octets a tout évincé : plus rien à annuler"
    );
    assert_eq!(h.len(), 1, "il doit rester exactement le dernier cran");
    assert!(h.undo(&mut p).unwrap());
}

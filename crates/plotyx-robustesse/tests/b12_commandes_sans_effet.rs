//! Batterie 12 — les commandes qui réussissent sans faire ce qu'elles disent.
//!
//! Deux motifs opposés, éprouvés ici au niveau du bus de commandes :
//!
//! - une commande **acceptée qui ne change rien** et occupe pourtant un cran
//!   d'annulation — l'utilisateur voit un succès, l'historique une entrée, et
//!   le modèle rien du tout ;
//! - une commande **acceptée qui change plus que ce qu'elle écrit** — un bloc
//!   de zéro valeur qui allonge quand même la colonne.
//!
//! Aucune des deux n'est un plantage ; les deux mentent à l'utilisateur.

use plotyx_commands::{Command, History};
use plotyx_core::{Column, NodeId, Project, ValueBlock};

/// `[Book1]Sheet1` vide, prête à recevoir des colonnes.
fn feuille() -> (Project, NodeId) {
    let mut project = Project::new("Demo").expect("projet valide");
    let root = project.root();
    project.add_workbook(root, Some("Book1")).expect("classeur");
    let sheet = project.resolve_path("[Book1]Sheet1").expect("feuille");
    (project, sheet)
}

fn longueur(project: &Project, column: NodeId) -> usize {
    project
        .tree()
        .node(column)
        .and_then(plotyx_core::Node::as_column)
        .map_or(0, Column::len)
}

fn valeurs(project: &Project, column: NodeId) -> Vec<f64> {
    project
        .tree()
        .node(column)
        .and_then(plotyx_core::Node::as_column)
        .and_then(Column::float_values)
        .expect("colonne numérique")
        .as_slice()
        .to_vec()
}

// -- garde-fous -----------------------------------------------------------

#[test]
fn b12_insert_rows_allonge_les_colonnes_qui_possedent_la_ligne() {
    let (mut project, sheet) = feuille();
    let a = project
        .add_column(sheet, None, Some("A"), Column::from_f64(&[1.0, 2.0, 3.0]))
        .expect("colonne A");
    let mut history = History::with_limit(0);
    history
        .execute(
            &mut project,
            Command::InsertRows {
                sheet,
                at: 1,
                count: 2,
            },
        )
        .expect("insertion acceptée");
    assert_eq!(longueur(&project, a), 5);
    assert!(valeurs(&project, a)[1].is_nan());
    assert!(history.undo(&mut project).expect("annulation"));
    assert_eq!(valeurs(&project, a), vec![1.0, 2.0, 3.0]);
}

#[test]
fn b12_delete_rows_reste_annulable_sur_des_colonnes_de_longueurs_differentes() {
    let (mut project, sheet) = feuille();
    let a = project
        .add_column(
            sheet,
            None,
            Some("A"),
            Column::from_f64(&[1.0, 2.0, 3.0, 4.0, 5.0]),
        )
        .expect("colonne A");
    let b = project
        .add_column(
            sheet,
            None,
            Some("B"),
            Column::from_f64(&[10.0, 20.0, 30.0]),
        )
        .expect("colonne B");
    let mut history = History::with_limit(0);
    // `count` déborde la fin de B : la suppression est rabattue, sans erreur.
    history
        .execute(
            &mut project,
            Command::DeleteRows {
                sheet,
                at: 2,
                count: 10,
            },
        )
        .expect("suppression acceptée");
    assert_eq!(longueur(&project, a), 2);
    assert_eq!(longueur(&project, b), 2);
    assert!(history.undo(&mut project).expect("annulation"));
    assert_eq!(valeurs(&project, a), vec![1.0, 2.0, 3.0, 4.0, 5.0]);
    assert_eq!(valeurs(&project, b), vec![10.0, 20.0, 30.0]);
}

#[test]
fn b12_setcells_ecrit_bien_au_dela_de_la_fin() {
    let (mut project, sheet) = feuille();
    let a = project
        .add_column(sheet, None, Some("A"), Column::from_f64(&[1.0]))
        .expect("colonne A");
    let mut history = History::with_limit(0);
    history
        .execute(
            &mut project,
            Command::SetCells {
                column: a,
                start: 3,
                values: ValueBlock::Float(vec![4.0]),
            },
        )
        .expect("écriture acceptée");
    assert_eq!(longueur(&project, a), 4);
    assert_eq!(valeurs(&project, a)[3], 4.0);
    assert!(history.undo(&mut project).expect("annulation"));
    assert_eq!(valeurs(&project, a), vec![1.0]);
}

// -- reproductions --------------------------------------------------------

#[test]
fn b12_une_commande_sans_effet_noccupe_pas_un_cran_dannulation() {
    // B5 (`guide-de-correction-robustesse.md` §F5) a deux moitiés. Celle-ci
    // couvre la cible vide : une feuille sans colonne, où `InsertRows` ne
    // peut par construction toucher personne. L'ancien scénario ici — une
    // feuille avec une seule colonne neuve, donc vide, à `at: 0` — testait
    // en réalité l'AUTRE moitié de B5 (`row_targets` refusait `at == len`) :
    // avec cette moitié-là corrigée (voir `Project::insert_rows`) et
    // `plotyx-app/src/app.rs` qui ne code plus `at: 0` en dur, cette
    // commande a désormais un effet réel — une ligne est ajoutée — et
    // MÉRITE son entrée d'historique ; ce n'est donc plus le bon scénario
    // pour ce test.
    let (mut project, sheet) = feuille();
    let mut history = History::with_limit(0);

    let resultat = history.execute(
        &mut project,
        Command::InsertRows {
            sheet,
            at: 0,
            count: 1,
        },
    );
    println!("InsertRows(at=0, count=1) sur une feuille sans colonne → {resultat:?}");
    println!(
        "lignes de la feuille = {}, entrées d'historique = {} ({:?})",
        project.tree().row_count(sheet).unwrap_or(0),
        history.len(),
        history.undo_label()
    );
    assert!(
        resultat.is_err() || history.is_empty(),
        "la commande a réussi, n'a rien changé (aucune colonne à toucher), et a pourtant \
         empilé « {:?} » : l'utilisateur lit un succès, le menu Annuler propose une \
         action qui ne défera rien, et un historique de 100 crans en perd un vrai",
        history.undo_label()
    );
}

#[test]
fn b12_une_ligne_peut_etre_ajoutee_a_la_fin_dune_feuille() {
    let (mut project, sheet) = feuille();
    let a = project
        .add_column(sheet, None, Some("A"), Column::from_f64(&[1.0, 2.0]))
        .expect("colonne A");
    let mut history = History::with_limit(0);
    let lignes = project.tree().row_count(sheet).unwrap_or(0);

    let resultat = history.execute(
        &mut project,
        Command::InsertRows {
            sheet,
            at: lignes,
            count: 1,
        },
    );
    println!("InsertRows(at={lignes}, count=1) → {resultat:?}");
    println!("longueur de A après = {}", longueur(&project, a));
    assert_eq!(
        project.tree().row_count(sheet).unwrap_or(0),
        lignes + 1,
        "insérer à la position `len` est la façon d'ajouter une ligne en fin de feuille \
         (c'est ce que `Column::insert_rows` accepte, `at ∈ 0..=len`) ; au niveau feuille \
         le filtre `at < len` de `row_targets` la refuse en silence, et aucune autre \
         commande ne le fait"
    );
}

#[test]
fn b12_setcells_dun_bloc_vide_ne_change_rien() {
    let (mut project, sheet) = feuille();
    let a = project
        .add_column(sheet, None, Some("A"), Column::from_f64(&[1.0, 2.0, 3.0]))
        .expect("colonne A");
    let mut history = History::with_limit(0);
    let avant = valeurs(&project, a);

    history
        .execute(
            &mut project,
            Command::SetCells {
                column: a,
                start: 10,
                values: ValueBlock::Float(Vec::new()),
            },
        )
        .expect("écriture acceptée");

    let apres = valeurs(&project, a);
    println!("avant : {avant:?}");
    println!("après SetCells(start = 10, 0 valeur) : {apres:?}");
    assert_eq!(
        apres, avant,
        "une commande qui n'écrit aucune valeur ne doit pas changer les données ; \
         `write_block` fait pourtant `resize_with(start, missing)` avant de découvrir \
         qu'il n'y a rien à écrire"
    );
}

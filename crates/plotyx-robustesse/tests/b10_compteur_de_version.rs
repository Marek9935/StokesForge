//! Batterie 10 — le contrat du compteur de version d'une colonne.
//!
//! `Column::version` est le seul mécanisme du socle qui réponde à « cette
//! colonne a-t-elle changé depuis la dernière fois que je l'ai lue ? » sans
//! relire les valeurs. Sa documentation fixe le sens de l'approximation
//! tolérée : « un faux *a changé* coûte un recalcul inutile, un faux *n'a pas
//! changé* afficherait un résultat faux. Seule la première erreur est
//! tolérable. »
//!
//! Cette batterie éprouve exactement cette phrase, au niveau du bus de
//! commandes plutôt qu'au niveau de la colonne (les tests unitaires de
//! `column.rs` couvrent déjà la colonne seule, et ils passent).

use plotyx_commands::{Command, History};
use plotyx_core::{Column, ColumnMeta, ColumnType, MixedCell, NodeId, Project, ValueBlock};

/// `[Book1]Sheet1` avec `A` = [1, 2, 3] et `B` = [10].
fn projet() -> (Project, NodeId, NodeId, NodeId) {
    let mut project = Project::new("Demo").expect("projet valide");
    let root = project.root();
    project.add_workbook(root, Some("Book1")).expect("classeur");
    let sheet = project.resolve_path("[Book1]Sheet1").expect("feuille");
    let a = project
        .add_column(sheet, None, Some("A"), Column::from_f64(&[1.0, 2.0, 3.0]))
        .expect("colonne A");
    let b = project
        .add_column(sheet, None, Some("B"), Column::from_f64(&[10.0]))
        .expect("colonne B");
    (project, sheet, a, b)
}

fn version(project: &Project, column: NodeId) -> u64 {
    project
        .tree()
        .node(column)
        .and_then(plotyx_core::Node::as_column)
        .expect("colonne")
        .version()
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

fn ecrire(history: &mut History, project: &mut Project, column: NodeId, valeur: f64) {
    history
        .execute(
            project,
            Command::SetCells {
                column,
                start: 0,
                values: ValueBlock::Float(vec![valeur]),
                expected_version: None,
            },
        )
        .expect("écriture acceptée");
}

// -- garde-fous -----------------------------------------------------------

#[test]
fn b10_le_compteur_avance_a_chaque_ecriture() {
    let (mut project, _sheet, a, _b) = projet();
    let mut history = History::with_limit(0);
    let depart = version(&project, a);
    ecrire(&mut history, &mut project, a, 9.0);
    let apres = version(&project, a);
    assert!(
        apres > depart,
        "le compteur doit avancer : {depart} puis {apres}"
    );
}

#[test]
fn b10_une_colonne_non_touchee_garde_son_compteur() {
    let (mut project, _sheet, a, b) = projet();
    let mut history = History::with_limit(0);
    let avant = version(&project, b);
    ecrire(&mut history, &mut project, a, 9.0);
    assert_eq!(
        version(&project, b),
        avant,
        "écrire dans A ne doit pas faire bouger le compteur de B"
    );
}

#[test]
fn b10_les_metadonnees_ne_sont_pas_des_donnees() {
    let (mut project, _sheet, a, _b) = projet();
    let avant = version(&project, a);
    project
        .set_column_meta(
            a,
            ColumnMeta {
                unit: "ms".to_string(),
                ..ColumnMeta::default()
            },
        )
        .expect("métadonnées acceptées");
    assert_eq!(
        version(&project, a),
        avant,
        "une unité n'est pas une donnée"
    );
}

#[test]
fn b10_une_commande_refusee_nincremente_rien() {
    let (mut project, _sheet, a, _b) = projet();
    let mut history = History::with_limit(0);
    let avant = version(&project, a);
    // Bloc mixte sur une colonne numérique : `Err(TypeMismatch)`.
    let refus = history.execute(
        &mut project,
        Command::SetCells {
            column: a,
            start: 0,
            values: ValueBlock::Mixed(vec![MixedCell::Text("x".to_string())]),
            expected_version: None,
        },
    );
    assert!(refus.is_err(), "la commande doit être refusée");
    assert_eq!(version(&project, a), avant);
}

#[test]
fn b10_le_compteur_traverse_un_changement_de_type() {
    let (mut project, _sheet, a, _b) = projet();
    let mut history = History::with_limit(0);
    let avant = version(&project, a);
    history
        .execute(
            &mut project,
            Command::SetColumnType {
                column: a,
                column_type: ColumnType::Mixed,
            },
        )
        .expect("conversion acceptée");
    assert!(
        version(&project, a) > avant,
        "une conversion est une écriture"
    );
}

#[test]
fn b10_la_saisie_tolerante_qui_promeut_une_colonne_est_une_ecriture() {
    // `Command::SetCellsPromoting` sur une colonne numérique qui reçoit un
    // texte : la colonne est promue en `Mixed` (chemin 3, voir `command.rs`)
    // *et* la cellule est écrite dans la même entrée d'historique. Les deux
    // effets doivent avancer le compteur, comme n'importe quelle écriture.
    let (mut project, _sheet, a, _b) = projet();
    let mut history = History::with_limit(0);
    let avant = version(&project, a);
    history
        .execute(
            &mut project,
            Command::SetCellsPromoting {
                column: a,
                start: 0,
                values: ValueBlock::Mixed(vec![MixedCell::Text("n/a".to_string())]),
                expected_version: None,
            },
        )
        .expect("saisie tolérante acceptée");
    let apres_promotion = version(&project, a);
    assert!(
        apres_promotion > avant,
        "promouvoir une colonne en Mixed et y écrire est une écriture : {avant} puis \
         {apres_promotion}"
    );

    // Une deuxième saisie tolérante, sur une colonne déjà mixte (chemin 1/2,
    // pas de promotion) : elle aussi doit avancer le compteur, et ne jamais
    // revenir à une version déjà vue.
    history
        .execute(
            &mut project,
            Command::SetCellsPromoting {
                column: a,
                start: 1,
                values: ValueBlock::Mixed(vec![MixedCell::Text("hs".to_string())]),
                expected_version: None,
            },
        )
        .expect("deuxième saisie tolérante acceptée");
    let apres_deuxieme = version(&project, a);
    assert!(
        apres_deuxieme > apres_promotion,
        "la saisie tolérante sur une colonne déjà mixte doit aussi avancer le compteur : \
         {apres_promotion} puis {apres_deuxieme}"
    );
}

// -- reproductions --------------------------------------------------------

#[test]
fn b10_le_compteur_de_version_ne_recule_jamais() {
    let (mut project, _sheet, a, _b) = projet();
    let mut history = History::with_limit(0);

    ecrire(&mut history, &mut project, a, 9.0);
    // Une commande de STRUCTURE : elle ne touche pas aux données, mais son
    // entrée d'historique retient l'arbre entier — colonnes comprises, donc
    // compteurs compris.
    history
        .execute(
            &mut project,
            Command::Rename {
                id: a,
                name: "A2".to_string(),
            },
        )
        .expect("renommage accepté");
    ecrire(&mut history, &mut project, a, 5.0);

    let sommet = version(&project, a);
    history.undo(&mut project).expect("annulation données");
    let apres_donnees = version(&project, a);
    history.undo(&mut project).expect("annulation structure");
    let apres_structure = version(&project, a);

    println!(
        "sommet = {sommet}, après annulation des données = {apres_donnees}, \
         après annulation de la structure = {apres_structure}"
    );
    assert!(
        apres_structure >= apres_donnees,
        "le compteur est passé de {apres_donnees} à {apres_structure} : il a reculé, \
         alors que sa documentation en fait le témoin monotone d'une modification"
    );
}

#[test]
fn b10_deux_contenus_ne_partagent_jamais_un_numero_de_version() {
    let (mut project, _sheet, a, _b) = projet();
    let mut history = History::with_limit(0);

    ecrire(&mut history, &mut project, a, 9.0);
    history
        .execute(
            &mut project,
            Command::Rename {
                id: a,
                name: "A2".to_string(),
            },
        )
        .expect("renommage accepté");

    // Ce que verrait un lecteur qui retient (version, contenu) — c'est
    // littéralement l'usage décrit par la documentation de `Column::version`.
    ecrire(&mut history, &mut project, a, 5.0);
    let version_retenue = version(&project, a);
    let contenu_retenu = valeurs(&project, a);

    history.undo(&mut project).expect("annulation données");
    history.undo(&mut project).expect("annulation structure");

    // Une écriture neuve (le rétablissement est perdu : `execute` vide la
    // pile de rétablissement), avec une AUTRE valeur.
    ecrire(&mut history, &mut project, a, 7.0);
    let version_courante = version(&project, a);
    let contenu_courant = valeurs(&project, a);

    println!("retenu   : version {version_retenue} → {contenu_retenu:?}");
    println!("courant  : version {version_courante} → {contenu_courant:?}");
    assert!(
        version_courante != version_retenue || contenu_courant == contenu_retenu,
        "la version {version_retenue} désigne maintenant {contenu_courant:?} alors qu'un \
         lecteur l'a retenue pour {contenu_retenu:?} : il se croira à jour et affichera \
         des valeurs fausses — le seul faux négatif que la documentation dit intolérable"
    );
}

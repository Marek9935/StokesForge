//! Batterie 15 — la traduction cellule ↔ texte, dans les deux sens.
//!
//! C'est la seule frontière du dépôt où une valeur du modèle devient une
//! chaîne destinée à un humain, puis redevient une valeur. Trois façons de s'y
//! tromper, une par test de cette batterie :
//!
//! - **perdre une distinction** que le modèle tient et que le format promet
//!   (chaîne vide présente vs valeur absente) ;
//! - **écrire alors qu'on n'a rien saisi** (une traversée de cellule est une
//!   commande) ;
//! - **rendre illisible** ce que le logiciel existe pour montrer (une mesure
//!   physique de l'ordre de 10⁻¹⁹).
//!
//! `AppState` ne dépend pas d'egui (voir `plotyx-app/tests/shell.rs`) : tout
//! ce qui suit tourne sans serveur graphique.

use plotyx_app::state::AppState;
use plotyx_core::{Cell, Column, ColumnType, ColumnValues, NodeId, Project};
use plotyx_session::{Session, SessionHandle};

/// `[Book1]Sheet1` avec une colonne texte `T` = [« a », « », absent] et une
/// colonne numérique `N` portant trois valeurs de physicien.
fn banc() -> (Session, SessionHandle, NodeId, NodeId) {
    let mut project = Project::new("Demo").expect("projet valide");
    let root = project.root();
    project.add_workbook(root, Some("Book1")).expect("classeur");
    let sheet = project.resolve_path("[Book1]Sheet1").expect("feuille");
    let texte = project
        .add_column(
            sheet,
            None,
            Some("T"),
            Column::from_text(&[Some("a".to_string()), Some(String::new()), None]),
        )
        .expect("colonne texte");
    let nombres = project
        .add_column(
            sheet,
            None,
            Some("N"),
            // charge de l'électron, nombre d'Avogadro, un flottant ordinaire
            Column::from_f64(&[1.602_176_634e-19, 6.022_140_76e23, 0.1 + 0.2]),
        )
        .expect("colonne numérique");
    let session = Session::spawn(project, None).expect("acteur démarré");
    let handle = session.handle();
    (session, handle, texte, nombres)
}

fn cellule(state: &AppState, column: NodeId, row: usize) -> Cell {
    state
        .tree()
        .node(column)
        .and_then(plotyx_core::Node::as_column)
        .expect("colonne")
        .get(row)
}

fn version(state: &AppState, column: NodeId) -> u64 {
    state
        .tree()
        .node(column)
        .and_then(plotyx_core::Node::as_column)
        .expect("colonne")
        .version()
}

// -- garde-fous -----------------------------------------------------------

#[test]
fn b15_une_saisie_numerique_fait_laller_retour() {
    let (_session, handle, _texte, nombres) = banc();
    let mut state = AppState::new(handle, "x.plx".to_string()).expect("état");
    for row in 0..3 {
        let affiche = state.cell_text(nombres, row);
        let attendu = cellule(&state, nombres, row);
        state.begin_edit(nombres, row);
        state.commit_edit();
        assert_eq!(
            cellule(&state, nombres, row),
            attendu,
            "la ligne {row} (« {affiche} ») doit revenir bit à bit"
        );
    }
}

#[test]
fn b15_une_cellule_numerique_vidée_devient_manquante_pas_zero() {
    let (_session, handle, _texte, nombres) = banc();
    let mut state = AppState::new(handle, "x.plx".to_string()).expect("état");
    state.begin_edit(nombres, 0);
    if let Some(edit) = state.editing_mut() {
        edit.buffer.clear();
    }
    state.commit_edit();
    assert_eq!(cellule(&state, nombres, 0), Cell::Float(f64::NAN));
}

#[test]
fn b15_un_texte_qui_nest_pas_un_nombre_est_refuse_sans_ecrire() {
    let (_session, handle, _texte, nombres) = banc();
    let mut state = AppState::new(handle, "x.plx".to_string()).expect("état");
    let avant = cellule(&state, nombres, 0);
    state.begin_edit(nombres, 0);
    if let Some(edit) = state.editing_mut() {
        edit.buffer = "pas un nombre".to_string();
    }
    state.commit_edit();
    assert_eq!(cellule(&state, nombres, 0), avant);
    assert!(state.status().is_some_and(|s| s.is_error));
}

// -- reproductions --------------------------------------------------------

#[test]
fn b15_une_chaine_vide_survit_a_la_traversee_de_sa_cellule() {
    let (_session, handle, texte, _nombres) = banc();
    let mut state = AppState::new(handle, "x.plx".to_string()).expect("état");

    assert_eq!(cellule(&state, texte, 1), Cell::Text(Some(String::new())));
    assert_eq!(cellule(&state, texte, 2), Cell::Text(None));
    // Les deux s'affichent pareil : rien, dans la grille, ne les distingue.
    assert_eq!(state.cell_text(texte, 1), "");
    assert_eq!(state.cell_text(texte, 2), "");

    // L'utilisateur clique dans la cellule 1 et clique ailleurs — ou passe au
    // clavier : `cell_widget` valide à **toute** perte de focus sauf Échap.
    state.begin_edit(texte, 1);
    state.commit_edit();

    println!("après traversée : {:?}", cellule(&state, texte, 1));
    assert_eq!(
        cellule(&state, texte, 1),
        Cell::Text(Some(String::new())),
        "la chaîne vide présente est devenue une valeur absente : le modèle les \
         distingue (`column.rs`), le format le promet depuis la version 1 (critère 3 \
         de `plotyx-persistence`), et la grille les confond dans les deux sens — \
         `cell_text` rend « » pour les deux, `commit_cell` rend `None` pour « »"
    );
}

#[test]
fn b15_traverser_une_cellule_sans_rien_taper_necrit_pas() {
    let (_session, handle, _texte, nombres) = banc();
    let mut state = AppState::new(handle, "x.plx".to_string()).expect("état");
    let revision = state.snapshot().revision;
    let compteur = version(&state, nombres);

    // Trois tabulations dans une colonne, sans une frappe.
    for row in 0..3 {
        state.begin_edit(nombres, row);
        state.commit_edit();
    }

    println!(
        "révision {revision} → {} ; version de la colonne {compteur} → {} ; annuler = {:?}",
        state.snapshot().revision,
        version(&state, nombres),
        state.undo_label()
    );
    assert_eq!(
        (state.snapshot().revision, version(&state, nombres)),
        (revision, compteur),
        "parcourir une ligne au clavier a produit trois commandes `SetCells`, trois \
         crans d'annulation « Saisie dans N » et trois incréments du compteur qui dit \
         aux lecteurs que la colonne a changé — alors que pas une valeur n'a bougé"
    );
}

#[test]
#[ignore = "reproduction C4 — le Display de f64 n'utilise jamais la notation scientifique : une mesure de l'ordre de 10⁻¹⁹ s'écrit sur 30 caractères"]
fn b15_une_mesure_scientifique_tient_dans_une_cellule() {
    let (_session, handle, _texte, nombres) = banc();
    let state = AppState::new(handle, "x.plx".to_string()).expect("état");

    // Une colonne de la grille fait 110 px de large : une quinzaine de
    // caractères. 24 est déjà large.
    const LISIBLE: usize = 24;
    let mut fautives = Vec::new();
    for (row, valeur) in [1.602_176_634e-19, 6.022_140_76e23, 0.1 + 0.2]
        .into_iter()
        .enumerate()
    {
        let affiche = state.cell_text(nombres, row);
        let scientifique = format!("{valeur:e}");
        println!(
            "{:>22} → {:>3} caractères ; en notation scientifique : {scientifique} ({} caractères)",
            scientifique,
            affiche.len(),
            scientifique.len()
        );
        if affiche.len() > LISIBLE {
            fautives.push((affiche.len(), affiche));
        }
    }
    assert!(
        fautives.is_empty(),
        "{fautives:?} : `cell_text` formate par le `Display` de `f64`, qui n'émet \
         jamais d'exposant — `f64::MIN_POSITIVE` s'écrirait sur 326 caractères. C'est \
         la forme de valeur pour laquelle ce logiciel existe"
    );
}

#[test]
#[ignore = "reproduction C4 — la même absence d'exposant multiplie par 7,6 le coût mémoire d'une conversion Numérique → Texte"]
fn b15_convertir_en_texte_ne_multiplie_pas_la_memoire() {
    use plotyx_commands::{Command, History};

    const LIGNES: usize = 50_000;
    let mut project = Project::new("Demo").expect("projet valide");
    let root = project.root();
    project.add_workbook(root, Some("Book1")).expect("classeur");
    let sheet = project.resolve_path("[Book1]Sheet1").expect("feuille");
    // Un courant mesuré, en ampères : l'ordre de grandeur du domaine.
    let mesures: Vec<f64> = (0..LIGNES)
        .map(|i| 1.602_176_634e-19 * (1.0 + i as f64 * 1e-9))
        .collect();
    let colonne = project
        .add_column(sheet, None, Some("I"), Column::from_f64(&mesures))
        .expect("colonne");

    let avant = LIGNES * std::mem::size_of::<f64>();
    let mut history = History::new();
    history
        .execute(
            &mut project,
            Command::SetColumnType {
                column: colonne,
                column_type: ColumnType::Text,
            },
        )
        .expect("conversion acceptée");

    let apres: usize = match project
        .tree()
        .node(colonne)
        .and_then(plotyx_core::Node::as_column)
        .expect("colonne")
        .values()
    {
        ColumnValues::Text(values) => values
            .iter()
            .map(|s| std::mem::size_of::<Option<String>>() + s.as_deref().map_or(0, str::len))
            .sum(),
        ColumnValues::Float(_) => 0,
    };
    let facteur = apres as f64 / avant as f64;
    println!("{LIGNES} lignes : {avant} octets → {apres} octets ({facteur:.1}×)");
    assert!(
        facteur <= 3.0,
        "une conversion vers texte multiplie la mémoire par {facteur:.1} parce que \
         chaque valeur devient sa forme décimale complète (30 caractères ici, jusqu'à \
         326 pour un dénormal) ; en notation scientifique le facteur plafonnerait \
         vers 3× — et l'historique en retient une copie de plus"
    );
}

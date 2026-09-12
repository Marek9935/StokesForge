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
    // A9 (option 2, déjà corrigée) : la chaîne vide présente et la valeur
    // absente s'affichent désormais différemment — voir
    // `b15_a9_option2_la_grille_distingue_absent_de_vide` pour la
    // reproduction dédiée à cette moitié-là de l'anomalie. Ce test-ci ne
    // porte que sur l'autre moitié (option 1) : la survie de `Some("")` à la
    // traversée d'une cellule.
    assert_eq!(state.cell_text(texte, 1), "");
    assert_ne!(state.cell_text(texte, 2), "");

    // L'utilisateur clique dans la cellule 1 et clique ailleurs — ou passe au
    // clavier : `cell_widget` valide à **toute** perte de focus sauf Échap.
    state.begin_edit(texte, 1);
    state.commit_edit();

    println!("après traversée : {:?}", cellule(&state, texte, 1));
    assert_eq!(
        cellule(&state, texte, 1),
        Cell::Text(Some(String::new())),
        "la chaîne vide présente est devenue une valeur absente à la traversée de sa \
         cellule : le modèle les distingue (`column.rs`), le format le promet depuis \
         la version 1 (critère 3 de `plotyx-persistence`) — corrigé dans `commit_cell` \
         (A9 option 1) : comparer le texte validé au texte déjà affiché avant de \
         dispatcher évite d'écrire quoi que ce soit quand rien n'a été tapé"
    );
}

/// A9 (option 2) — l'autre moitié de l'anomalie : même sans traversée, rien
/// ne distinguait à l'écran une chaîne vide présente d'une valeur absente
/// (`cell_text` rendait `""` pour les deux). Corrigé dans `state.rs::
/// cell_text` : une valeur absente s'affiche désormais comme un espace
/// insécable ([`plotyx_app::state::MISSING_TEXT_DISPLAY`]) plutôt que comme
/// une chaîne vide.
#[test]
fn b15_a9_option2_la_grille_distingue_absent_de_vide() {
    use plotyx_app::state::MISSING_TEXT_DISPLAY;

    let (_session, handle, texte, _nombres) = banc();
    let state = AppState::new(handle, "x.plx".to_string()).expect("état");

    assert_eq!(cellule(&state, texte, 1), Cell::Text(Some(String::new())));
    assert_eq!(cellule(&state, texte, 2), Cell::Text(None));

    let vide_presente = state.cell_text(texte, 1);
    let absente = state.cell_text(texte, 2);
    assert_eq!(
        vide_presente, "",
        "une chaîne vide présente reste affichée comme une chaîne vide"
    );
    assert_eq!(
        absente, MISSING_TEXT_DISPLAY,
        "une valeur absente s'affiche selon la convention retenue par `cell_text`"
    );
    assert_ne!(
        vide_presente, absente,
        "« {vide_presente:?} » (présente) et « {absente:?} » (absente) doivent \
         s'afficher différemment, sans quoi rien ne les distingue à l'écran"
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
#[ignore = "reproduction C4 (diagnostic révisé) — Column::set_column_type (Float→Text) utilise \
            désormais format_f64_for_display (notation scientifique quand elle est plus courte), \
            ce qui fait baisser le facteur mesuré de 7,6× à ~5,7× sur ce scénario, mais le plafond \
            de 3× que ce test demande est hors de portée d'un simple choix de format : \
            size_of::<Option<String>>() vaut 24 octets sur une machine 64 bits, contre 8 pour le \
            f64 qu'il remplace — un facteur 3,0 rien que pour la structure, AVANT même le premier \
            caractère de contenu. Aucune chaîne non vide ne peut donc jamais faire mieux que « un \
            peu plus de 3× » avec Vec<Option<String>> comme représentation de colonne texte ; \
            l'atteindre demanderait de changer cette représentation elle-même (une chaîne \
            interned/packée plutôt qu'un String par cellule), un chantier bien plus large que la \
            question de format que C4 posait"]
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

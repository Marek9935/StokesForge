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

/// Un texte qui n'est pas un nombre, tapé dans une colonne numérique, ne se
/// perd plus : il élargit la colonne en [`ColumnType::Mixed`] et devient une
/// cellule texte, les voisines gardant leurs nombres. **Ce qui doit tenir
/// ici, c'est qu'aucune donnée ne disparaît** — ni la saisie (qui était
/// refusée avant les colonnes à types mixtes), ni les valeurs déjà présentes
/// (qu'une conversion de toute la colonne en texte aurait rendues
/// intraçables).
#[test]
fn b15_un_texte_qui_nest_pas_un_nombre_elargit_la_colonne_sans_rien_perdre() {
    let (_session, handle, _texte, nombres) = banc();
    let mut state = AppState::new(handle, "x.plx".to_string()).expect("état");
    let voisine = cellule(&state, nombres, 1);
    state.begin_edit(nombres, 0);
    if let Some(edit) = state.editing_mut() {
        edit.buffer = "pas un nombre".to_string();
    }
    state.commit_edit();

    assert!(
        state.status().is_none_or(|s| !s.is_error),
        "ce n'est plus une erreur"
    );
    assert_eq!(
        state
            .tree()
            .node(nombres)
            .and_then(plotyx_core::Node::as_column)
            .expect("colonne")
            .column_type(),
        ColumnType::Mixed
    );
    assert_eq!(state.cell_text(nombres, 0), "pas un nombre");
    assert_eq!(
        cellule(&state, nombres, 1),
        voisine,
        "la ligne voisine garde son nombre, bit à bit"
    );

    // Et l'aller-retour d'affichage tient toujours sur les lignes numériques
    // d'une colonne devenue mixte : retaper ce qui est affiché n'écrit rien
    // qui change la valeur.
    let attendu = cellule(&state, nombres, 1);
    state.begin_edit(nombres, 1);
    state.commit_edit();
    assert_eq!(cellule(&state, nombres, 1), attendu);
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

/// C4 — plus une reproduction : un **choix d'architecture documenté**, pas
/// un bug. `size_of::<Option<String>>()` vaut 24 octets sur une machine 64
/// bits, contre 8 pour le `f64` qu'il remplace : un facteur 3,0 rien que
/// pour la structure, avant le premier caractère de contenu — c'est le
/// plancher mathématique de `Vec<Option<String>>` comme représentation
/// d'une colonne texte, et aucun choix de format ne peut descendre en
/// dessous. `Column::set_column_type` (Float→Text) utilise
/// `format_f64_for_display` (notation scientifique quand elle est plus
/// courte), qui atténue le coût mais ne peut pas effacer ce plancher ; pour
/// des valeurs à haute précision comme ici (grandeurs physiques, une
/// dizaine de chiffres significatifs), le contenu texte lui-même ajoute
/// encore dessus — le facteur mesuré tourne autour de 5-6×, pas 3×, sur ce
/// genre de données. Ce n'est toujours pas un problème : ×5-6 est un
/// multiplicateur **constant**, pas une croissance non bornée — ce test
/// vérifie justement que le facteur ne dérive pas avec la taille des
/// données (voir `facteur_stable_avec_la_taille` ci-dessous). Descendre
/// sous ce plancher demanderait de changer la représentation elle-même
/// (une chaîne interned/packée plutôt qu'un `String` par cellule) — un
/// chantier bien plus large que ce que C4 posait, non entrepris.
#[test]
fn b15_convertir_en_texte_multiplie_la_memoire_par_un_facteur_borne_et_stable() {
    use plotyx_commands::{Command, History};

    fn facteur(lignes: usize) -> f64 {
        let mut project = Project::new("Demo").expect("projet valide");
        let root = project.root();
        project.add_workbook(root, Some("Book1")).expect("classeur");
        let sheet = project.resolve_path("[Book1]Sheet1").expect("feuille");
        // Un courant mesuré, en ampères : l'ordre de grandeur du domaine.
        let mesures: Vec<f64> = (0..lignes)
            .map(|i| 1.602_176_634e-19 * (1.0 + i as f64 * 1e-9))
            .collect();
        let colonne = project
            .add_column(sheet, None, Some("I"), Column::from_f64(&mesures))
            .expect("colonne");

        let avant = lignes * std::mem::size_of::<f64>();
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
            // Ce test mesure la seule conversion `Numeric -> Text` : les
            // deux autres variantes n'arrivent jamais ici.
            ColumnValues::Float(_) | ColumnValues::Mixed(_) => 0,
        };
        apres as f64 / avant as f64
    }

    // Le plancher structurel ne bouge pas en dessous de 3,0 (voir la doc du
    // test) ; on laisse une marge généreuse au-dessus pour le contenu
    // réel sans pour autant tolérer une dérive qui indiquerait une
    // régression (ex. `format_f64_for_display` qui cesserait de préférer
    // la notation scientifique).
    let petit = facteur(5_000);
    let grand = facteur(50_000);
    println!("5 000 lignes : {petit:.2}× ; 50 000 lignes : {grand:.2}×");
    for (lignes, f) in [(5_000, petit), (50_000, grand)] {
        assert!(
            (3.0..=8.0).contains(&f),
            "{lignes} lignes : facteur {f:.2}×, hors de la plage attendue [3,0 ; 8,0] — \
             en dessous de 3,0 serait suspect (le plancher structurel de \
             `Option<String>` est infranchissable), au-dessus indiquerait une \
             régression du choix de format"
        );
    }
    assert!(
        (petit - grand).abs() / petit <= 0.15,
        "le facteur varie de {petit:.2}× à {grand:.2}× entre 5 000 et 50 000 lignes \
         (plus de 15% d'écart) : il devrait rester stable, puisque c'est un coût par \
         cellule, pas un effet qui s'aggrave avec la taille du projet"
    );
}

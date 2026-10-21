//! « Colonne utilisée par un calcul » (#306) : la fenêtre qui s'ouvre après
//! la suppression de colonnes lues par une formule ou un calcul d'analyse
//! (`Close` ne fait rien, `Undo` annule le geste ; le déplacement d'une colonne,
//! lui, n'ouvre rien : `column_move.rs`, #319), et la question de
//! Ctrl+Maj+C sur des colonnes à formule (valeurs ou formules).

use stokesforge_analysis::model::Model;
use stokesforge_app_state::analysis::Preset;
use stokesforge_app_state::i18n::{self, Language};
use stokesforge_app_state::state::{Action, AppState, FullColumnsCopy, HeaderField};
use stokesforge_core::{Column, ColumnMeta, ColumnRole, NodeId, Project};
use stokesforge_session::Session;

struct Setup {
    _session: Session,
    state: AppState,
    sheet: NodeId,
    cols: Vec<NodeId>,
}

/// Une feuille `A` (X), `B` (Y), `C`, `D` ordinaires, sur 5 lignes.
fn setup(dir: &std::path::Path) -> Setup {
    i18n::set_language(Language::English);
    let mut project = Project::new("P").unwrap();
    let book = project.add_workbook(project.root(), Some("Book")).unwrap();
    let sheet = project.tree().children(book).unwrap()[0];
    let values: Vec<f64> = (0..5).map(f64::from).collect();
    let cols: Vec<NodeId> = ["A", "B", "C", "D"]
        .iter()
        .map(|name| {
            project
                .add_column(sheet, None, Some(name), Column::from_f64(&values))
                .unwrap()
        })
        .collect();
    project
        .set_column_meta(
            cols[0],
            ColumnMeta {
                role: ColumnRole::X,
                ..ColumnMeta::default()
            },
        )
        .unwrap();
    let session = Session::spawn(project, None).unwrap();
    let state = AppState::new_with_preferences(
        session.handle(),
        dir.join("p.stf").display().to_string(),
        stokesforge_app_state::preferences::Preferences::load_from(None),
    )
    .unwrap();
    Setup {
        _session: session,
        state,
        sheet,
        cols,
    }
}

fn set_formula(s: &mut Setup, column: NodeId, text: &str) {
    s.state.begin_header_edit(column, HeaderField::Formula);
    s.state.header_editing_mut().unwrap().buffer = text.to_string();
    s.state.commit_header_edit();
}

fn delete(s: &mut Setup, columns: &[NodeId]) {
    s.state.select_column(s.sheet, columns[0]);
    for &c in &columns[1..] {
        s.state.toggle_column_selection(s.sheet, c);
    }
    s.state
        .dispatch(Action::DeleteSelectedCells { sheet: s.sheet });
}

fn column_count(s: &Setup) -> usize {
    s.state.tree().children(s.sheet).unwrap().len()
}

#[test]
fn supprimer_une_colonne_lue_par_une_formule_ouvre_la_liste_puis_close_ne_fait_rien() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = setup(dir.path());
    let c = s.cols.clone();
    set_formula(&mut s, c[2], "A*2");
    delete(&mut s, &[c[0]]);
    assert_eq!(
        s.state.column_use_lines(),
        Some(vec!["C (formula)".to_string()])
    );
    s.state.dispatch(Action::CloseColumnUse);
    assert_eq!(s.state.column_use_lines(), None);
    assert_eq!(column_count(&s), 3, "Close ne rend pas la colonne");
}

/// #321 : une formule retient les colonnes par identifiant, une lettre qui
/// glisse ne change donc rien à ce qu'elle lit — supprimer A n'avertit pas
/// `C = B*2` (qui s'affiche désormais `A*2`, B étant devenue la première
/// colonne). Le lecteur d'une colonne supprimée, lui, est toujours listé.
#[test]
fn les_lettres_qui_glissent_ne_comptent_pas_seule_la_colonne_lue_compte() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = setup(dir.path());
    let c = s.cols.clone();
    set_formula(&mut s, c[2], "B*2");
    delete(&mut s, &[c[0]]);
    assert_eq!(
        s.state.column_use_lines(),
        None,
        "C ne lit pas A : B change de lettre, pas de colonne"
    );
    assert_eq!(
        s.state.tree().formula_letters(c[2]).as_deref(),
        Some("A*2"),
        "la lettre affichée suit la colonne"
    );

    // Témoin : supprimer B, que C lit, ouvre la liste.
    delete(&mut s, &[c[1]]);
    assert_eq!(
        s.state.column_use_lines(),
        Some(vec!["C (formula)".to_string()])
    );
}

#[test]
fn une_colonne_sans_lecteur_n_ouvre_rien() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = setup(dir.path());
    let c = s.cols.clone();
    delete(&mut s, &[c[3]]);
    assert_eq!(s.state.column_use_lines(), None);
    assert_eq!(column_count(&s), 3);
}

#[test]
fn undo_rend_la_colonne_et_la_formule_en_un_pas_et_plusieurs_colonnes_font_une_liste() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = setup(dir.path());
    let c = s.cols.clone();
    set_formula(&mut s, c[3], "A+B");
    let before = s.state.column_meta(c[3]);
    delete(&mut s, &[c[0], c[1]]);
    assert_eq!(
        s.state.column_use_lines(),
        Some(vec!["D (formula)".to_string()]),
        "une seule liste pour tout le geste"
    );
    assert_eq!(column_count(&s), 2);
    s.state.dispatch(Action::UndoColumnUse);
    assert_eq!(s.state.column_use_lines(), None);
    assert_eq!(column_count(&s), 4, "un seul Undo rend les deux colonnes");
    assert_eq!(s.state.column_meta(c[3]), before);
}

#[test]
fn un_calcul_d_analyse_est_liste_par_sa_famille_et_sa_premiere_sortie() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = setup(dir.path());
    let c = s.cols.clone();
    s.state.dispatch(Action::OpenAnalysis {
        preset: Preset::Fit(Model::Linear),
        sheet: s.sheet,
        columns: vec![c[0], c[1]],
    });
    s.state.dispatch(Action::ApplyAnalysis);
    assert_eq!(s.state.tree().calculations().len(), 1);
    let first_output = s
        .state
        .tree()
        .calculations()
        .values()
        .next()
        .unwrap()
        .outputs
        .iter()
        .find_map(|o| o.column)
        .expect("une colonne de sortie");

    // Supprimer Y (B) : le calcul lit B, il est listé.
    delete(&mut s, &[c[1]]);
    let lines = s.state.column_use_lines().expect("fenêtre ouverte");
    assert_eq!(lines.len(), 1);
    assert!(lines[0].starts_with("Fit → [Book]Sheet1!"), "{lines:?}");
    s.state.dispatch(Action::UndoColumnUse);

    // Témoin : supprimer sa colonne de sortie n'ouvre rien (il disparaît avec).
    delete(&mut s, &[first_output]);
    assert_eq!(s.state.column_use_lines(), None);
}

#[test]
fn supprimer_un_classeur_emporte_ses_calculs_sans_rien_signaler() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = setup(dir.path());
    let c = s.cols.clone();
    s.state.dispatch(Action::OpenAnalysis {
        preset: Preset::Fit(Model::Linear),
        sheet: s.sheet,
        columns: vec![c[0], c[1]],
    });
    s.state.dispatch(Action::ApplyAnalysis);
    // Les sorties vivent dans le même classeur : il emporte le calcul avec
    // lui, rien à signaler.
    let book = s.state.tree().parent(s.sheet).unwrap().unwrap();
    s.state.dispatch(Action::DeleteObjects(vec![book]));
    assert!(s.state.tree().node(book).is_none(), "classeur supprimé");
    assert_eq!(s.state.column_use_lines(), None);
}

#[test]
fn ctrl_maj_c_sur_une_colonne_formule_pose_la_question() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = setup(dir.path());
    let c = s.cols.clone();
    set_formula(&mut s, c[2], "A*2");

    s.state.select_column(s.sheet, c[2]);
    assert_eq!(s.state.copy_full_columns(s.sheet), None);
    assert_eq!(s.state.pending_full_copy(), Some(s.sheet));
    s.state
        .dispatch(Action::ConfirmFullColumnsCopy(FullColumnsCopy::Values));
    let values = s.state.take_object_clipboard_text().expect("texte copié");
    assert!(!values.contains("A*2"), "la ligne Formule reste vide");
    assert_eq!(s.state.pending_full_copy(), None);

    s.state.select_column(s.sheet, c[2]);
    s.state.copy_full_columns(s.sheet);
    s.state
        .dispatch(Action::ConfirmFullColumnsCopy(FullColumnsCopy::Formulas));
    let formulas = s.state.take_object_clipboard_text().expect("texte copié");
    assert!(formulas.contains("A*2"));

    s.state.select_column(s.sheet, c[2]);
    s.state.copy_full_columns(s.sheet);
    s.state
        .dispatch(Action::ConfirmFullColumnsCopy(FullColumnsCopy::Cancel));
    assert_eq!(s.state.take_object_clipboard_text(), None);
}

#[test]
fn ctrl_maj_c_sans_formule_copie_directement() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = setup(dir.path());
    let c = s.cols.clone();
    s.state.select_column(s.sheet, c[1]);
    assert!(s.state.copy_full_columns(s.sheet).is_some());
    assert_eq!(s.state.pending_full_copy(), None);
}

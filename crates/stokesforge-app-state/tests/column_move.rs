//! Déplacer des colonnes (#319, sous-menu `Move ▸`) : un seul Ctrl+Z, aucune
//! fenêtre « Colonne utilisée » (une formule suit sa colonne, #321), la
//! sélection reste sur les colonnes déplacées. La logique d'ordre est testée à
//! part, sans état (`column_move::tests`).

use stokesforge_app_state::column_move::ColumnMove;
use stokesforge_app_state::i18n::{self, Language};
use stokesforge_app_state::state::{Action, AppState, HeaderField};
use stokesforge_core::{Column, ColumnMeta, ColumnRole, NodeId, Project};
use stokesforge_session::Session;

struct Setup {
    _session: Session,
    state: AppState,
    sheet: NodeId,
    cols: Vec<NodeId>,
}

/// Une feuille `A`, `B`, `C`, `D`, `E` de valeurs distinctes sur 3 lignes.
fn setup(dir: &std::path::Path) -> Setup {
    i18n::set_language(Language::English);
    let mut project = Project::new("P").unwrap();
    let book = project.add_workbook(project.root(), Some("Book")).unwrap();
    let sheet = project.tree().children(book).unwrap()[0];
    let cols: Vec<NodeId> = ["A", "B", "C", "D", "E"]
        .iter()
        .enumerate()
        .map(|(rank, name)| {
            let base = (rank as f64 + 1.0) * 10.0;
            let id = project
                .add_column(
                    sheet,
                    None,
                    Some(name),
                    Column::from_f64(&[base, base + 1.0, base + 2.0]),
                )
                .unwrap();
            project
                .set_column_meta(
                    id,
                    ColumnMeta {
                        role: ColumnRole::Y,
                        ..ColumnMeta::default()
                    },
                )
                .unwrap();
            id
        })
        .collect();
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

fn order(s: &Setup) -> Vec<NodeId> {
    s.state
        .visible_columns(s.sheet)
        .into_iter()
        .map(|(id, _, _)| id)
        .collect()
}

fn select(s: &mut Setup, columns: &[NodeId]) {
    s.state.select_column(s.sheet, columns[0]);
    for &column in &columns[1..] {
        s.state.toggle_column_selection(s.sheet, column);
    }
}

fn move_selection(s: &mut Setup, columns: &[NodeId], to: ColumnMove) {
    select(s, columns);
    s.state.dispatch(Action::MoveColumns {
        sheet: s.sheet,
        columns: columns.to_vec(),
        to,
    });
}

fn first_cell(s: &Setup, column: NodeId) -> String {
    s.state.cell_text(column, 0)
}

/// `A [B] C [D] E`, Left : `B A D C E`, en **un seul** Undo qui rend l'ordre
/// d'origine, libellé « Move Columns ». Témoin : sans macro, deux Undo.
#[test]
fn deux_colonnes_deplacees_font_une_seule_entree_d_historique() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = setup(dir.path());
    let c = s.cols.clone();
    move_selection(&mut s, &[c[1], c[3]], ColumnMove::Left);
    assert_eq!(order(&s), vec![c[1], c[0], c[3], c[2], c[4]]);
    let label = s.state.undo_label().expect("une entrée à annuler");
    assert!(label.contains("Move Columns"), "{label}");

    s.state.dispatch(Action::Undo);
    assert_eq!(order(&s), c, "un seul Undo rend l'ordre d'origine");
    // Rien de plus à annuler : la macro était la seule entrée.
    assert!(!s.state.can_undo());
}

/// Une seule colonne : une entrée `Move`, nommée.
#[test]
fn une_colonne_seule_est_une_entree_move_nommee() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = setup(dir.path());
    let c = s.cols.clone();
    move_selection(&mut s, &[c[1]], ColumnMove::Right);
    assert_eq!(order(&s), vec![c[0], c[2], c[1], c[3], c[4]]);
    let label = s.state.undo_label().unwrap();
    assert!(label.contains("Move “B”"), "{label}");
    s.state.dispatch(Action::Undo);
    assert_eq!(order(&s), c);
}

/// Un geste qui ne change rien n'écrit rien (l'entrée du menu est grisée).
#[test]
fn un_geste_sans_effet_n_ecrit_rien() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = setup(dir.path());
    let c = s.cols.clone();
    let revision = s.state.revision();
    move_selection(&mut s, &[c[0], c[2]], ColumnMove::Left);
    move_selection(&mut s, &[c[3], c[4]], ColumnMove::Last);
    assert_eq!(order(&s), c);
    assert_eq!(s.state.revision(), revision);
    assert!(!s.state.can_undo());
}

/// Une formule lit une colonne ; la déplacer n'ouvre aucune fenêtre « Colonne
/// utilisée », et la formule garde ses valeurs. Témoin : appeler
/// `open_column_use` dans `move_columns`.
#[test]
fn deplacer_une_colonne_lue_par_une_formule_n_ouvre_aucune_fenetre() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = setup(dir.path());
    let c = s.cols.clone();
    // C = A*2
    s.state.begin_header_edit(c[2], HeaderField::Formula);
    s.state.header_editing_mut().unwrap().buffer = "A*2".to_string();
    s.state.commit_header_edit();
    let value = first_cell(&s, c[2]);
    assert_eq!(value, "20", "formule posée");

    move_selection(&mut s, &[c[0]], ColumnMove::Right);

    assert_eq!(s.state.column_use_lines(), None, "aucune fenêtre");
    assert_eq!(first_cell(&s, c[2]), value, "valeurs de la formule gardées");
    assert_eq!(
        s.state.tree().formula_letters(c[2]).as_deref(),
        Some("B*2"),
        "la lettre affichée suit la colonne lue"
    );
    // Lire une autre colonne que celle qui bouge : tout aussi muet.
    move_selection(&mut s, &[c[1]], ColumnMove::First);
    assert_eq!(s.state.column_use_lines(), None);
    assert_eq!(first_cell(&s, c[2]), value);
}

/// Après le déplacement, la sélection couvre les mêmes colonnes (par identifiant,
/// y compris une plage).
#[test]
fn la_selection_reste_sur_les_colonnes_deplacees() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = setup(dir.path());
    let c = s.cols.clone();
    for (columns, to) in [
        (vec![c[1], c[3]], ColumnMove::Left),
        (vec![c[1], c[3]], ColumnMove::First),
        (vec![c[1], c[3]], ColumnMove::Last),
        (vec![c[1], c[3]], ColumnMove::Right),
    ] {
        move_selection(&mut s, &columns, to);
        let selected = s
            .state
            .selection(s.sheet)
            .expect("la sélection survit")
            .columns(s.state.tree(), s.sheet);
        let mut expected = columns.clone();
        expected.sort_by_key(|id| order(&s).iter().position(|o| o == id));
        assert_eq!(selected, expected, "{to:?}");
        s.state.dispatch(Action::Undo);
    }
    // Une plage (Maj-clic) B..C bouge d'un bloc : toujours sélectionnée.
    s.state.select_column(s.sheet, c[1]);
    s.state.extend_column_selection(s.sheet, c[2]);
    s.state.dispatch(Action::MoveColumns {
        sheet: s.sheet,
        columns: vec![c[1], c[2]],
        to: ColumnMove::Last,
    });
    assert_eq!(order(&s), vec![c[0], c[3], c[4], c[1], c[2]]);
    let selected = s
        .state
        .selection(s.sheet)
        .unwrap()
        .columns(s.state.tree(), s.sheet);
    assert_eq!(selected, vec![c[1], c[2]]);
}

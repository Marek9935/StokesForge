//! The sheet of a new workbook per file is named in the interface language at
//! import time, and keeps that language afterwards (#488). A file of its own:
//! the language loader is global.

use stokesforge_app_state::i18n::{self, Language};
use stokesforge_app_state::state::{Action, AppState, ImportDestination};
use stokesforge_core::{Column, Project};
use stokesforge_import::ImportRecipe;
use stokesforge_session::Session;

#[test]
fn the_french_sheet_is_donnees_brutes_then_feuille1() {
    let _language = i18n::test_language(Language::French);
    let dir = tempfile::tempdir().expect("temporary directory");
    let mut project = Project::new("Demo").expect("project");
    let root = project.root();
    let book = project.add_workbook(root, Some("Book1")).expect("book");
    let sheet = project.tree().children(book).expect("sheets")[0];
    project
        .add_column(sheet, None, Some("A"), Column::from_f64(&[1.0]))
        .expect("column");
    let session = Session::spawn(project, None).expect("session");
    let mut state = AppState::new_with_preferences(
        session.handle(),
        dir.path().join("p.stf").display().to_string(),
        stokesforge_app_state::preferences::Preferences::load_from(None),
    )
    .expect("state");
    let file = dir.path().join("mesure.csv");
    std::fs::write(&file, "1,10\n2,20\n").unwrap();
    state.dispatch(Action::ImportFiles {
        destination: ImportDestination::NewBookPerFile,
        recipe: ImportRecipe::new("Untitled"),
        paths: vec![file],
    });
    let tree = state.tree();
    let imported = tree
        .walk_all()
        .into_iter()
        .find(|id| tree.node(*id).is_some_and(|n| n.name() == "mesure"))
        .expect("the workbook takes the name of the file");
    let sheet = state.worksheets_of(imported)[0].0;
    assert_eq!(state.tree().node(sheet).unwrap().name(), "Données brutes");

    // The language changes afterwards: the base was stored with the mark.
    i18n::set_language(Language::English);
    let column = state.visible_columns(sheet)[0].0;
    state.select_cell(sheet, column, 0);
    state.dispatch(Action::Paste {
        sheet,
        text: "5\n".to_string(),
    });
    assert_eq!(state.tree().node(sheet).unwrap().name(), "Feuille1");
}

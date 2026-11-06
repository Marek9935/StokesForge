//! The signal "a dialog is open in the main window" (#471): every state that
//! paints a dialog answers, none answers `None`, and the refusal sentence
//! follows the way the dialog ends.

use stokesforge_app_state::analysis::Preset;
use stokesforge_app_state::i18n::{self, Language};
use stokesforge_app_state::state::open_dialog::{refusal_text, DialogKind, OpenDialog};
use stokesforge_app_state::state::{
    Action, AppState, PlotSettingsTab, UnsavedChoice, UnsavedGesture,
};
use stokesforge_core::{Column, NodeId, Project};
use stokesforge_session::Session;

struct Setup {
    _session: Session,
    state: AppState,
    sheet: NodeId,
    x: NodeId,
    y: NodeId,
}

fn setup(dir: &std::path::Path) -> Setup {
    let mut project = Project::new("P").unwrap();
    let book = project.add_workbook(project.root(), Some("Book")).unwrap();
    let sheet = project.tree().children(book).unwrap()[0];
    let values: Vec<f64> = (0..5).map(f64::from).collect();
    let x = project
        .add_column(sheet, None, Some("X"), Column::from_f64(&values))
        .unwrap();
    let y = project
        .add_column(sheet, None, Some("Y"), Column::from_f64(&values))
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
        x,
        y,
    }
}

/// Opens one dialog by its own gesture and returns what the signal should say.
fn open(s: &mut Setup, which: OpenDialog) {
    match which {
        OpenDialog::About => s.state.dispatch(Action::OpenAbout),
        OpenDialog::Preferences => s.state.dispatch(Action::OpenPreferences),
        OpenDialog::KeyboardShortcuts => s.state.dispatch(Action::OpenKeyboardShortcuts),
        OpenDialog::FormulaFunctions => s.state.dispatch(Action::OpenFormulaFunctions),
        OpenDialog::StatusHistory => s.state.dispatch(Action::OpenStatusHistory),
        OpenDialog::ColumnMetadata => s
            .state
            .dispatch(Action::ViewColumnMetadata { column: s.y }),
        OpenDialog::Analysis => s.state.dispatch(Action::OpenAnalysis {
            preset: Preset::Integrate,
            sheet: s.sheet,
            columns: vec![s.x, s.y],
        }),
        OpenDialog::ImportWizard => s.state.open_import(None),
        OpenDialog::Question => s.state.dispatch(Action::NewProject {
            pending_edits: false,
        }),
        OpenDialog::GraphProperties => {
            s.state.dispatch(Action::AddGraph);
            let root = s.state.tree().root();
            let graph = *s.state.tree().children(root).unwrap().last().unwrap();
            s.state.dispatch(Action::OpenPlotSettings {
                graph,
                tab: PlotSettingsTab::Graph,
                axis: None,
            });
        }
        // Opened by a gesture that needs a scenario of its own (low R² fits,
        // recovery copies, column in use): covered by the exhaustive `kind`.
        OpenDialog::LowR2List | OpenDialog::ColumnUse | OpenDialog::Recovery => unreachable!(),
    }
}

#[test]
fn each_dialog_answers_and_nothing_open_answers_none() {
    let _language = i18n::test_language(Language::English);
    let dir = tempfile::tempdir().unwrap();
    for which in [
        OpenDialog::About,
        OpenDialog::Preferences,
        OpenDialog::KeyboardShortcuts,
        OpenDialog::FormulaFunctions,
        OpenDialog::StatusHistory,
        OpenDialog::ColumnMetadata,
        OpenDialog::Analysis,
        OpenDialog::ImportWizard,
        OpenDialog::GraphProperties,
        OpenDialog::Question,
    ] {
        let mut s = setup(dir.path());
        assert_eq!(s.state.open_dialog(), None, "{which:?}: nothing open yet");
        open(&mut s, which);
        assert_eq!(s.state.open_dialog(), Some(which));
    }
}

#[test]
fn a_question_wins_over_the_dialog_under_it() {
    let _language = i18n::test_language(Language::English);
    let dir = tempfile::tempdir().unwrap();
    let mut s = setup(dir.path());
    open(&mut s, OpenDialog::About);
    open(&mut s, OpenDialog::Question);
    assert_eq!(s.state.open_dialog(), Some(OpenDialog::Question));
    assert_eq!(s.state.unsaved_pending(), Some(&UnsavedGesture::NewProject));
    s.state.dispatch(Action::ConfirmUnsaved(UnsavedChoice::Cancel));
    assert_eq!(s.state.open_dialog(), Some(OpenDialog::About));
    s.state.dispatch(Action::CloseAbout);
    assert_eq!(s.state.open_dialog(), None);
}

#[test]
fn the_sentence_follows_the_way_the_dialog_ends() {
    let _language = i18n::test_language(Language::English);
    assert_eq!(OpenDialog::GraphProperties.kind(), DialogKind::Editing);
    assert_eq!(OpenDialog::About.kind(), DialogKind::CloseOnly);
    assert_eq!(OpenDialog::Question.kind(), DialogKind::Question);
    assert_eq!(
        refusal_text(OpenDialog::GraphProperties),
        "Graph Properties is open in the main window. Apply or cancel it first"
    );
    assert_eq!(
        refusal_text(OpenDialog::About),
        "About StokesForge is open in the main window. Close it first"
    );
    assert_eq!(
        refusal_text(OpenDialog::Question),
        "A question is waiting in the main window. Answer it first"
    );
}

#[test]
fn a_refusal_reaches_status_and_history_without_touching_the_dialog_footer() {
    let _language = i18n::test_language(Language::English);
    let dir = tempfile::tempdir().unwrap();
    let mut s = setup(dir.path());
    // A correction without a reference cannot be validated: its error is in
    // the footer of the open window.
    s.state.dispatch(Action::OpenAnalysis {
        preset: Preset::Correct,
        sheet: s.sheet,
        columns: vec![s.x, s.y],
    });
    s.state.dispatch(Action::ApplyAnalysis);
    let footer = s.state.dialog_error().map(str::to_owned);
    assert!(footer.is_some());

    let before = s.state.recorded_count();
    let text = s.state.refuse_outside_main(OpenDialog::Analysis);
    assert_eq!(
        text,
        "Analysis is open in the main window. Apply or cancel it first"
    );
    assert_eq!(s.state.dialog_error().map(str::to_owned), footer);
    assert_eq!(s.state.recorded_count(), before + 1);
    let last = s.state.status().unwrap();
    assert_eq!(last.text, text);
    assert_eq!(last.level, stokesforge_app_state::state::StatusLevel::Warning);

    // The same sentence again: no new history line.
    let lines = s.state.status_history().count();
    s.state.refuse_outside_main(OpenDialog::Analysis);
    assert_eq!(s.state.status_history().count(), lines);
    assert_eq!(s.state.recorded_count(), before + 1);
}

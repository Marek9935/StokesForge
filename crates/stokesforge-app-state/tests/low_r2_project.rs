//! The count of low R² for a whole project (#290), end to end from the
//! state: a calculation tree built by hand (12 reconvolutions and 1 fit with
//! R² < 0.6, plus calculations that do not count), opened in a session, then
//! read by `AppState`. The real scenario (fit then reconvolution recalculated)
//! would cost an IRF and nonlinear fits for nothing more proven about the
//! count; the recalculation cases live in `analysis_state.rs`.

use stokesforge_analysis::correct::CorrectOp;
use stokesforge_analysis::deconv::ReconvSpec;
use stokesforge_analysis::fit::FitSpec;
use stokesforge_analysis::model::Model;
use stokesforge_analysis::ops::{Operation, OperationKind};
use stokesforge_app_state::i18n::{self, Language};
use stokesforge_app_state::r_squared::R_SQUARED_KEY;
use stokesforge_app_state::state::{Action, AppState};
use stokesforge_core::{
    CalculationDraft, Column, NodeId, Project, ResultValue, SeriesRef, SeriesResult,
};
use stokesforge_session::Session;

fn series_result(r2: f64) -> SeriesResult {
    SeriesResult {
        values: vec![
            ResultValue {
                key: "param.tau".into(),
                index: None,
                value: 0.1,
                error: None,
            },
            ResultValue {
                key: R_SQUARED_KEY.into(),
                index: None,
                value: r2,
                error: None,
            },
        ],
        ..Default::default()
    }
}

fn reconvolution() -> Operation {
    Operation::Reconvolution {
        spec: ReconvSpec {
            components: 1,
            params: Vec::new(),
            shift_initial: 0.0,
            fit_shift: false,
            window: None,
        },
        residuals: false,
    }
}

fn fit_operation() -> Operation {
    Operation::Fit {
        spec: FitSpec {
            model: Model::Linear,
            params: Vec::new(),
        },
        range: None,
        residuals: false,
    }
}

struct World {
    project: Project,
    sheet: NodeId,
    n: usize,
}

impl World {
    fn new() -> Self {
        let mut project = Project::new("P").unwrap();
        let book = project.add_workbook(project.root(), Some("Book")).unwrap();
        let sheet = project.tree().children(book).unwrap()[0];
        Self {
            project,
            sheet,
            n: 0,
        }
    }

    /// Adds a calculation of `operation` whose series have the given `r2`.
    fn add_calculation(&mut self, operation: Operation, r2: &[f64], keep: bool, dialog: bool) {
        self.n += 1;
        let series: Vec<SeriesRef> = r2
            .iter()
            .map(|_| {
                self.n += 1;
                let y = self
                    .project
                    .add_column(
                        self.sheet,
                        None,
                        Some(&format!("Y{}", self.n)),
                        Column::from_f64(&[1.0, 2.0]),
                    )
                    .unwrap();
                SeriesRef {
                    x: None,
                    y,
                    sigma: None,
                }
            })
            .collect();
        self.project
            .add_calculation(CalculationDraft {
                operation,
                series,
                aux: None,
                outputs: Vec::new(),
                results: if keep {
                    r2.iter().map(|r| series_result(*r)).collect()
                } else {
                    Vec::new()
                },
                keep_results: keep,
                auto_recalc: false,
                low_r2_dialog: dialog,
            })
            .unwrap();
    }

    fn open_in(self, dir: &std::path::Path) -> (Session, AppState, i18n::LanguageGuard) {
        let language = i18n::test_language(Language::English);
        let session = Session::spawn(self.project, None).unwrap();
        let state = AppState::new_with_preferences(
            session.handle(),
            dir.join("p.stf").display().to_string(),
            stokesforge_app_state::preferences::Preferences::load_from(None),
        )
        .unwrap();
        (session, state, language)
    }
}

fn groups_of(state: &AppState) -> Vec<(OperationKind, usize)> {
    state
        .low_r2_groups()
        .iter()
        .map(|g| (g.kind, g.count))
        .collect()
}

/// A single test: the language is global to this binary.
#[test]
fn twelve_reconvolutions_and_one_fit_read_end_to_end() {
    let dir = tempfile::tempdir().unwrap();
    let mut world = World::new();
    // The fit first in the tree, the reconvolutions after: the order of
    // appearance does not decide the order of the rows.
    world.add_calculation(fit_operation(), &[0.3], true, true);
    for k in 0..12 {
        // A `low_r2_dialog` setting turned off on half of them: the badge counts them.
        world.add_calculation(
            reconvolution(),
            &[0.1 + f64::from(k) * 0.01],
            true,
            k % 2 == 0,
        );
    }
    // What must not count.
    world.add_calculation(fit_operation(), &[0.95, 0.6, f64::NAN], true, true); // good, right at the threshold, NaN
    world.add_calculation(reconvolution(), &[0.2], false, true); // "Show": nothing kept
    world.add_calculation(
        Operation::Correct {
            op: CorrectOp::Subtract,
        },
        &[0.99],
        true,
        true,
    );
    let (_session, mut state, _language) = world.open_in(dir.path());

    assert_eq!(
        groups_of(&state),
        vec![(OperationKind::Fit, 1), (OperationKind::Reconvolution, 12)]
    );
    assert_eq!(state.low_r2_total(), 13);
    assert_eq!(
        state.low_r2_list_text(),
        "R² < 0.6 for:\n• 1 fit\n• 12 reconvolutions\nSee the Results pane for the values."
    );
    assert_eq!(
        stokesforge_app_state::r_squared::badge_text(state.low_r2_total()),
        "13 low R²"
    );
    // Opening a project is not a recalculation: no dialog, but the badge
    // and its list work.
    assert!(state.alert().is_none());
    state.dispatch(Action::OpenLowR2List);
    assert!(state.low_r2_list_open());
    assert!(state.alert().is_none(), "the list is not an alert");
    state.dispatch(Action::CloseLowR2List);
    state.dispatch(Action::CloseLowR2List);
    assert!(!state.low_r2_list_open(), "closing twice: idempotent");
    assert_eq!(state.low_r2_total(), 13, "closing the list empties nothing");
}

#[test]
fn the_count_is_per_series_and_not_per_calculation() {
    let dir = tempfile::tempdir().unwrap();
    let mut world = World::new();
    // A calculation with 3 series, 2 of them at fault.
    world.add_calculation(fit_operation(), &[0.1, 0.9, 0.2], true, true);
    let (_session, state, _language) = world.open_in(dir.path());
    assert_eq!(groups_of(&state), vec![(OperationKind::Fit, 2)]);
    assert_eq!(state.low_r2_total(), 2);
}

#[test]
fn a_project_without_a_low_r2_has_neither_group_nor_list() {
    let dir = tempfile::tempdir().unwrap();
    let mut world = World::new();
    world.add_calculation(fit_operation(), &[0.99, 0.6], true, true);
    let (_session, mut state, _language) = world.open_in(dir.path());
    assert!(state.low_r2_groups().is_empty());
    assert_eq!(state.low_r2_total(), 0);
    // The badge has nothing to list: opening has no effect.
    state.dispatch(Action::OpenLowR2List);
    assert!(!state.low_r2_list_open());
}

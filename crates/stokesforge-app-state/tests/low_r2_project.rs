//! Le décompte des R² faibles d'un projet entier (#290), de bout en bout depuis
//! l'état : un arbre de calculs bâti à la main (12 reconvolutions et 1
//! ajustement à R² < 0,6, plus des calculs qui ne comptent pas), ouvert dans
//! une session, puis lu par `AppState`. Le scénario réel (ajustement puis
//! reconvolution recalculés) coûterait une IRF et des ajustements non linéaires
//! pour ne rien prouver de plus sur le décompte ; les cas de recalcul vivent
//! dans `analysis_state.rs`.

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

fn resultat(r2: f64) -> SeriesResult {
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

fn ajustement() -> Operation {
    Operation::Fit {
        spec: FitSpec {
            model: Model::Linear,
            params: Vec::new(),
        },
        range: None,
        residuals: false,
    }
}

struct Monde {
    project: Project,
    sheet: NodeId,
    n: usize,
}

impl Monde {
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

    /// Ajoute un calcul de `operation` dont les séries ont les `r2` donnés.
    fn calcul(&mut self, operation: Operation, r2: &[f64], keep: bool, dialog: bool) {
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
                    r2.iter().map(|r| resultat(*r)).collect()
                } else {
                    Vec::new()
                },
                keep_results: keep,
                auto_recalc: false,
                low_r2_dialog: dialog,
            })
            .unwrap();
    }

    fn ouvre(self, dir: &std::path::Path) -> (Session, AppState) {
        i18n::set_language(Language::English);
        let session = Session::spawn(self.project, None).unwrap();
        let state = AppState::new_with_preferences(
            session.handle(),
            dir.join("p.stf").display().to_string(),
            stokesforge_app_state::preferences::Preferences::load_from(None),
        )
        .unwrap();
        (session, state)
    }
}

fn groupes(state: &AppState) -> Vec<(OperationKind, usize)> {
    state
        .low_r2_groups()
        .iter()
        .map(|g| (g.kind, g.count))
        .collect()
}

/// Un seul test : la langue est globale à ce binaire.
#[test]
fn douze_reconvolutions_et_un_ajustement_se_lisent_de_bout_en_bout() {
    let dir = tempfile::tempdir().unwrap();
    let mut monde = Monde::new();
    // L'ajustement d'abord dans l'arbre, les reconvolutions ensuite : l'ordre
    // d'apparition ne décide pas de l'ordre des lignes.
    monde.calcul(ajustement(), &[0.3], true, true);
    for k in 0..12 {
        // Un réglage `low_r2_dialog` coupé sur la moitié : la pastille les compte.
        monde.calcul(
            reconvolution(),
            &[0.1 + f64::from(k) * 0.01],
            true,
            k % 2 == 0,
        );
    }
    // Ce qui ne doit pas compter.
    monde.calcul(ajustement(), &[0.95, 0.6, f64::NAN], true, true); // bons, pile au seuil, NaN
    monde.calcul(reconvolution(), &[0.2], false, true); // « Show » : rien gardé
    monde.calcul(
        Operation::Correct {
            op: CorrectOp::Subtract,
        },
        &[0.99],
        true,
        true,
    );
    let (_session, mut state) = monde.ouvre(dir.path());

    assert_eq!(
        groupes(&state),
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
    // Ouvrir un projet n'est pas un recalcul : pas de boîte, mais la pastille
    // et sa liste fonctionnent.
    assert!(state.alert().is_none());
    state.dispatch(Action::OpenLowR2List);
    assert!(state.low_r2_list_open());
    assert!(state.alert().is_none(), "la liste n'est pas une alerte");
    state.dispatch(Action::CloseLowR2List);
    state.dispatch(Action::CloseLowR2List);
    assert!(!state.low_r2_list_open(), "fermer deux fois : idempotent");
    assert_eq!(state.low_r2_total(), 13, "fermer la liste ne vide rien");
}

#[test]
fn le_comptage_est_par_serie_et_non_par_calcul() {
    let dir = tempfile::tempdir().unwrap();
    let mut monde = Monde::new();
    // Un calcul à 3 séries dont 2 en défaut.
    monde.calcul(ajustement(), &[0.1, 0.9, 0.2], true, true);
    let (_session, state) = monde.ouvre(dir.path());
    assert_eq!(groupes(&state), vec![(OperationKind::Fit, 2)]);
    assert_eq!(state.low_r2_total(), 2);
}

#[test]
fn un_projet_sans_r2_faible_n_a_ni_groupe_ni_liste() {
    let dir = tempfile::tempdir().unwrap();
    let mut monde = Monde::new();
    monde.calcul(ajustement(), &[0.99, 0.6], true, true);
    let (_session, mut state) = monde.ouvre(dir.path());
    assert!(state.low_r2_groups().is_empty());
    assert_eq!(state.low_r2_total(), 0);
    // La pastille n'a rien à lister : l'ouverture est sans effet.
    state.dispatch(Action::OpenLowR2List);
    assert!(!state.low_r2_list_open());
}

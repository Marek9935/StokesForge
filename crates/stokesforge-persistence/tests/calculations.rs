//! Les calculs d'analyse (#181) traversent le fichier `.stf`.

use stokesforge_analysis::correct::CorrectOp;
use stokesforge_analysis::ops::{Operation, Warning};
use stokesforge_core::{
    CalculationDraft, Column, ColumnMeta, OutputDraft, Project, ResultValue, SeriesRef,
    SeriesResult, Staleness,
};
use stokesforge_persistence::{load, save};

fn project_with_calculation() -> Project {
    let mut project = Project::new("P").unwrap();
    let book = project.add_workbook(project.root(), Some("Book")).unwrap();
    let sheet = project.tree().children(book).unwrap()[0];
    let x = project
        .add_column(sheet, None, Some("X"), Column::from_f64(&[1.0, 2.0, 3.0]))
        .unwrap();
    let y = project
        .add_column(
            sheet,
            None,
            Some("Y"),
            Column::from_f64(&[10.0, 20.0, 30.0]),
        )
        .unwrap();
    project
        .add_calculation(CalculationDraft {
            operation: Operation::Correct {
                op: CorrectOp::Subtract,
            },
            series: vec![SeriesRef {
                x: Some(x),
                y,
                sigma: None,
            }],
            aux: None,
            outputs: vec![OutputDraft {
                series: 0,
                key: "corrected".into(),
                index: None,
                meta: ColumnMeta::default(),
                data: vec![9.0, 18.0, f64::NAN],
            }],
            results: vec![SeriesResult {
                values: vec![
                    ResultValue {
                        key: "count.outside".into(),
                        index: None,
                        value: 1.0,
                        error: None,
                    },
                    ResultValue {
                        key: "param.a".into(),
                        index: Some(2),
                        value: f64::NAN,
                        error: Some(0.5),
                    },
                ],
                skipped: 3,
                warnings: vec![Warning::NegativeClipped(4), Warning::IrfStepMismatch],
            }],
            keep_results: true,
            auto_recalc: false,
        })
        .unwrap();
    project
}

#[test]
fn un_calcul_survit_a_un_aller_retour() {
    let project = project_with_calculation();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("p.stf");
    save(&project, &path).unwrap();
    let back = load(&path).unwrap();
    assert_eq!(back.tree().calculations().len(), 1);
    assert_eq!(back.tree(), project.tree());
}

#[test]
fn la_peremption_manuelle_et_l_entree_perdue_sont_gardees_l_echec_devient_manuel() {
    for (stale, expected) in [
        (Staleness::Manual, Staleness::Manual),
        (Staleness::InputMissing, Staleness::InputMissing),
        (
            Staleness::Failed(stokesforge_analysis::AnalysisError::EmptyRange),
            Staleness::Manual,
        ),
    ] {
        let mut project = project_with_calculation();
        let id = *project.tree().calculations().keys().next().unwrap();
        project.set_calculation_stale(id, Some(stale)).unwrap();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("p.stf");
        save(&project, &path).unwrap();
        let back = load(&path).unwrap();
        assert_eq!(back.tree().calculation(id).unwrap().stale, Some(expected));
    }
}

#[test]
fn les_identifiants_continuent_apres_un_rechargement() {
    let project = project_with_calculation();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("p.stf");
    save(&project, &path).unwrap();
    let mut back = load(&path).unwrap();
    let id = *back.tree().calculations().keys().next().unwrap();
    let book = back.tree().children(back.root()).unwrap()[0];
    let fresh = back.add_workbook(back.root(), Some("Autre")).unwrap();
    assert_ne!(fresh, id);
    assert_ne!(fresh, book);
}

#[test]
fn le_reglage_d_avertissement_d_un_graphe_survit_a_un_aller_retour() {
    for wanted in [true, false] {
        let mut project = project_with_calculation();
        let spec = stokesforge_plot::PlotSpec {
            stale_warning: wanted,
            ..Default::default()
        };
        let graph = project.add_graph(project.root(), None, spec).unwrap();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("p.stf");
        save(&project, &path).unwrap();
        let back = load(&path).unwrap();
        let stokesforge_core::NodeKind::Graph { spec, .. } =
            back.tree().node(graph).unwrap().kind()
        else {
            panic!("un graphe attendu");
        };
        assert_eq!(spec.stale_warning, wanted);
    }
}

#[test]
fn le_separateur_decimal_d_un_graphe_survit_a_un_aller_retour_et_vaut_point_par_defaut() {
    for wanted in [true, false] {
        let mut project = project_with_calculation();
        let spec = stokesforge_plot::PlotSpec {
            decimal_comma: wanted,
            ..Default::default()
        };
        let graph = project.add_graph(project.root(), None, spec).unwrap();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("p.stf");
        save(&project, &path).unwrap();
        let back = load(&path).unwrap();
        let stokesforge_core::NodeKind::Graph { spec, .. } =
            back.tree().node(graph).unwrap().kind()
        else {
            panic!("un graphe attendu");
        };
        assert_eq!(spec.decimal_comma, wanted);
    }
    assert!(!stokesforge_plot::PlotSpec::default().decimal_comma);
}

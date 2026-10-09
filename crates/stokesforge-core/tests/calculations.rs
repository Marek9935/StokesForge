//! Les calculs d'analyse du modèle (#181) : création de colonnes à la bonne
//! place, mise à jour en place, retrait, écritures hors historique, égalité des
//! arbres malgré des `NaN`.

use stokesforge_analysis::correct::CorrectOp;
use stokesforge_analysis::ops::Operation;
use stokesforge_core::{
    CalculationDraft, Column, ColumnMeta, CoreError, NodeId, OutputDraft, Project, SeriesRef,
    SeriesResult, Staleness,
};

struct Fixture {
    project: Project,
    sheet: NodeId,
    x: NodeId,
    y1: NodeId,
    y2: NodeId,
}

fn fixture() -> Fixture {
    let mut project = Project::new("P").unwrap();
    let book = project.add_workbook(project.root(), Some("Book")).unwrap();
    let sheet = project.tree().children(book).unwrap()[0];
    let x = project
        .add_column(sheet, None, Some("X"), Column::from_f64(&[1.0, 2.0, 3.0]))
        .unwrap();
    let y1 = project
        .add_column(
            sheet,
            None,
            Some("Y1"),
            Column::from_f64(&[10.0, 20.0, 30.0]),
        )
        .unwrap();
    let y2 = project
        .add_column(sheet, None, Some("Y2"), Column::from_f64(&[5.0, 6.0, 7.0]))
        .unwrap();
    Fixture {
        project,
        sheet,
        x,
        y1,
        y2,
    }
}

fn names(f: &Fixture) -> Vec<String> {
    f.project
        .tree()
        .children(f.sheet)
        .unwrap()
        .iter()
        .map(|c| f.project.tree().get(*c).unwrap().name().to_string())
        .collect()
}

fn out(series: usize, key: &str, data: Vec<f64>, long: &str) -> OutputDraft {
    OutputDraft {
        series,
        key: key.to_string(),
        index: None,
        meta: ColumnMeta {
            long_name: long.to_string(),
            ..Default::default()
        },
        data,
    }
}

fn draft(f: &Fixture, outputs: Vec<OutputDraft>) -> CalculationDraft {
    CalculationDraft {
        operation: Operation::Correct {
            op: CorrectOp::Divide,
        },
        series: vec![
            SeriesRef {
                x: Some(f.x),
                y: f.y1,
                sigma: None,
            },
            SeriesRef {
                x: Some(f.x),
                y: f.y2,
                sigma: None,
            },
        ],
        aux: None,
        outputs,
        results: vec![SeriesResult::default(), SeriesResult::default()],
        keep_results: true,
        auto_recalc: true,
    }
}

#[test]
fn les_colonnes_de_sortie_suivent_chacune_leur_colonne_y() {
    let mut f = fixture();
    let d = draft(
        &f,
        vec![
            out(0, "corrected", vec![1.0, 2.0, 3.0], "Y1 (corr.)"),
            out(0, "fit", vec![4.0, 5.0, 6.0], "Y1 (fit)"),
            out(1, "corrected", vec![7.0, 8.0, 9.0], "Y2 (corr.)"),
        ],
    );
    let id = f.project.add_calculation(d).unwrap();
    // X, Y1, [2 sorties de Y1], Y2, [1 sortie de Y2] ; noms automatiques libres.
    assert_eq!(f.project.tree().children(f.sheet).unwrap().len(), 6);
    let kids = f.project.tree().children(f.sheet).unwrap().to_vec();
    assert_eq!(kids[0], f.x);
    assert_eq!(kids[1], f.y1);
    assert_eq!(kids[4], f.y2);
    let calc = f.project.tree().calculation(id).unwrap().clone();
    assert_eq!(calc.outputs[0].column, Some(kids[2]));
    assert_eq!(calc.outputs[1].column, Some(kids[3]));
    assert_eq!(calc.outputs[2].column, Some(kids[5]));
    // La trace se lit depuis la colonne.
    assert_eq!(
        f.project.tree().calculation_of_column(kids[3]),
        Some((id, 1))
    );
    assert_eq!(f.project.tree().calculation_of_column(f.y1), None);
    let col = f.project.tree().get(kids[2]).unwrap().as_column().unwrap();
    assert_eq!(col.meta().long_name, "Y1 (corr.)");
    assert_eq!(col.numeric_view().unwrap().as_slice(), &[1.0, 2.0, 3.0]);
    assert_eq!(names(&f).len(), 6);
}

#[test]
fn un_echec_laisse_l_arbre_intact() {
    let mut f = fixture();
    let before = f.project.tree().clone();
    // Série inconnue.
    let mut d = draft(&f, vec![out(0, "corrected", vec![1.0], "a")]);
    d.series[0].y = NodeId::from_raw(99_999);
    assert!(f.project.add_calculation(d).is_err());
    // Sortie qui vise une série absente.
    let d = draft(&f, vec![out(5, "corrected", vec![1.0], "a")]);
    assert!(matches!(
        f.project.add_calculation(d),
        Err(CoreError::Structure(_))
    ));
    // Aucune série.
    let mut d = draft(&f, vec![]);
    d.series.clear();
    d.results.clear();
    assert!(f.project.add_calculation(d).is_err());
    assert_eq!(f.project.tree(), &before);
}

#[test]
fn les_identifiants_de_calcul_ne_sont_jamais_reutilises() {
    let mut f = fixture();
    let a = f.project.add_calculation(draft(&f, vec![])).unwrap();
    f.project.remove_calculation(a, false).unwrap();
    let b = f.project.add_calculation(draft(&f, vec![])).unwrap();
    assert_ne!(a, b);
    assert!(b.raw() > a.raw());
    // Un projet rechargé ne réattribue pas non plus l'identifiant d'un calcul.
    let tree = f.project.tree().clone();
    let mut reloaded = Project::from_tree(tree, 0);
    let c = reloaded.add_calculation(draft(&f, vec![])).unwrap();
    assert!(c.raw() > b.raw(), "{c:?} doit dépasser {b:?}");
}

#[test]
fn la_mise_a_jour_reecrit_en_place_cree_et_supprime() {
    let mut f = fixture();
    let id = f
        .project
        .add_calculation(draft(
            &f,
            vec![
                out(0, "corrected", vec![1.0, 2.0, 3.0], "c"),
                out(0, "fit", vec![4.0, 5.0, 6.0], "f"),
            ],
        ))
        .unwrap();
    let kids = f.project.tree().children(f.sheet).unwrap().to_vec();
    let (corrected, fit) = (kids[2], kids[3]);
    // « fit » disparaît, « residuals » apparaît, « corrected » est réécrite.
    f.project
        .update_calculation(
            id,
            draft(
                &f,
                vec![
                    out(0, "corrected", vec![9.0, 9.0, 9.0], "c2"),
                    out(0, "residuals", vec![0.5, 0.5, 0.5], "r"),
                ],
            ),
        )
        .unwrap();
    assert!(
        f.project.tree().node(fit).is_none(),
        "colonne retirée avec sa sortie"
    );
    let calc = f.project.tree().calculation(id).unwrap().clone();
    assert_eq!(
        calc.outputs[0].column,
        Some(corrected),
        "réécrite en place, même colonne"
    );
    let col = f
        .project
        .tree()
        .get(corrected)
        .unwrap()
        .as_column()
        .unwrap();
    assert_eq!(col.numeric_view().unwrap().as_slice(), &[9.0, 9.0, 9.0]);
    assert_eq!(col.meta().long_name, "c2");
    let res = calc.outputs[1].column.unwrap();
    assert_eq!(f.project.tree().calculation_of_column(res), Some((id, 1)));
    assert_eq!(f.project.tree().children(f.sheet).unwrap().len(), 5);
}

#[test]
fn retirer_un_calcul_garde_ou_supprime_ses_colonnes() {
    let mut f = fixture();
    let id = f
        .project
        .add_calculation(draft(
            &f,
            vec![out(0, "corrected", vec![1.0, 2.0, 3.0], "c")],
        ))
        .unwrap();
    let produced = f.project.tree().calculation(id).unwrap().outputs[0]
        .column
        .unwrap();
    f.project.remove_calculation(id, false).unwrap();
    assert!(f.project.tree().calculation(id).is_none());
    assert!(
        f.project.tree().node(produced).is_some(),
        "détachée : colonne ordinaire"
    );
    assert_eq!(f.project.tree().calculation_of_column(produced), None);

    let id = f
        .project
        .add_calculation(draft(
            &f,
            vec![out(0, "corrected", vec![1.0, 2.0, 3.0], "c")],
        ))
        .unwrap();
    let produced = f.project.tree().calculation(id).unwrap().outputs[0]
        .column
        .unwrap();
    f.project.remove_calculation(id, true).unwrap();
    assert!(f.project.tree().node(produced).is_none());
    assert!(f.project.remove_calculation(id, true).is_err());
}

#[test]
fn le_rafraichissement_ne_reecrit_que_ce_qui_change() {
    let mut f = fixture();
    let id = f
        .project
        .add_calculation(draft(
            &f,
            vec![out(0, "corrected", vec![1.0, f64::NAN, 3.0], "c")],
        ))
        .unwrap();
    let produced = f.project.tree().calculation(id).unwrap().outputs[0]
        .column
        .unwrap();
    let v0 = f
        .project
        .tree()
        .get(produced)
        .unwrap()
        .as_column()
        .unwrap()
        .version();
    // Mêmes valeurs (NaN compris) : aucune écriture, donc aucune cascade.
    f.project
        .refresh_calculation(id, &[Some(vec![1.0, f64::NAN, 3.0])], vec![])
        .unwrap();
    assert_eq!(
        f.project
            .tree()
            .get(produced)
            .unwrap()
            .as_column()
            .unwrap()
            .version(),
        v0
    );
    // Valeurs différentes : écrites, version neuve.
    f.project
        .refresh_calculation(id, &[Some(vec![1.0, 2.0, 3.0, 4.0])], vec![])
        .unwrap();
    let col = f.project.tree().get(produced).unwrap().as_column().unwrap();
    assert!(col.version() > v0);
    assert_eq!(col.numeric_view().unwrap().len(), 4);
    // `None` laisse la colonne.
    f.project.refresh_calculation(id, &[None], vec![]).unwrap();
    assert_eq!(
        f.project
            .tree()
            .get(produced)
            .unwrap()
            .as_column()
            .unwrap()
            .numeric_view()
            .unwrap()
            .len(),
        4
    );
}

#[test]
fn l_etat_perime_se_pose_et_se_leve() {
    let mut f = fixture();
    let id = f.project.add_calculation(draft(&f, vec![])).unwrap();
    f.project
        .set_calculation_stale(id, Some(Staleness::Manual))
        .unwrap();
    assert_eq!(
        f.project.tree().calculation(id).unwrap().stale,
        Some(Staleness::Manual)
    );
    f.project.refresh_calculation(id, &[], vec![]).unwrap();
    assert_eq!(f.project.tree().calculation(id).unwrap().stale, None);
    assert!(f
        .project
        .set_calculation_stale(NodeId::from_raw(424_242), None)
        .is_err());
}

#[test]
fn des_resultats_nan_ne_cassent_pas_l_egalite_des_arbres() {
    let mut f = fixture();
    let mut d = draft(&f, vec![]);
    d.results = vec![
        SeriesResult {
            values: vec![stokesforge_core::ResultValue {
                key: "stat.r_squared".into(),
                index: None,
                value: f64::NAN,
                error: None,
            }],
            ..Default::default()
        },
        SeriesResult::default(),
    ];
    let id = f.project.add_calculation(d).unwrap();
    let a = f.project.tree().clone();
    let calc = f.project.tree().calculation(id).unwrap().as_ref().clone();
    f.project.restore_calculation(id, Some(calc));
    assert_eq!(&a, f.project.tree(), "un NaN égale un NaN dans un résultat");
}

#[test]
fn sans_garder_les_resultats_rien_n_est_stocke() {
    let mut f = fixture();
    let mut d = draft(&f, vec![out(0, "corrected", vec![1.0, 2.0, 3.0], "c")]);
    d.keep_results = false;
    d.results = vec![
        SeriesResult {
            skipped: 3,
            ..Default::default()
        },
        SeriesResult::default(),
    ];
    let id = f.project.add_calculation(d).unwrap();
    let calc = f.project.tree().calculation(id).unwrap();
    assert!(calc.results.is_empty() && !calc.keep_results);
    f.project
        .refresh_calculation(
            id,
            &[None],
            vec![SeriesResult {
                skipped: 9,
                ..Default::default()
            }],
        )
        .unwrap();
    assert!(f.project.tree().calculation(id).unwrap().results.is_empty());
}

#[test]
fn supprimer_les_entrees_retire_le_calcul_orphelin_mais_garde_un_calcul_partiel() {
    let mut f = fixture();
    let id = f
        .project
        .add_calculation(draft(
            &f,
            vec![out(0, "corrected", vec![1.0, 2.0, 3.0], "c")],
        ))
        .unwrap();
    // Une seule des deux séries perd son Y : le calcul reste.
    f.project.remove(f.y2).unwrap();
    assert!(f.project.tree().calculation(id).is_some());
    // La seconde aussi : plus rien à calculer.
    f.project.remove(f.y1).unwrap();
    assert!(f.project.tree().calculation(id).is_none());
}

#[test]
fn restaurer_l_arbre_d_avant_une_suppression_ramene_le_calcul() {
    let mut f = fixture();
    let id = f
        .project
        .add_calculation(draft(
            &f,
            vec![out(0, "corrected", vec![1.0, 2.0, 3.0], "c")],
        ))
        .unwrap();
    let before = f.project.tree().clone();
    f.project.remove(f.y2).unwrap();
    f.project.remove(f.y1).unwrap();
    assert!(f.project.tree().calculation(id).is_none());
    f.project.replace_tree(before.clone());
    assert_eq!(f.project.tree(), &before);
    assert!(f.project.tree().calculation(id).is_some());
}

// ── compute : la lecture du modèle partagée par la création et le recalcul ──

use stokesforge_core::{compute, AuxRef, ComputeError};

#[test]
fn compute_lit_les_colonnes_et_aligne_les_sorties_sur_les_lignes() {
    let f = fixture();
    // Référence : Y constant = 2 sur le même X → Divide donne y / 2.
    let mut project = f.project;
    let rx = project
        .add_column(
            f.sheet,
            None,
            Some("RX"),
            Column::from_f64(&[1.0, 2.0, 3.0]),
        )
        .unwrap();
    let ry = project
        .add_column(
            f.sheet,
            None,
            Some("RY"),
            Column::from_f64(&[2.0, 2.0, 2.0]),
        )
        .unwrap();
    let computed = compute(
        project.tree(),
        &Operation::Correct {
            op: CorrectOp::Divide,
        },
        &[SeriesRef {
            x: Some(f.x),
            y: f.y1,
            sigma: None,
        }],
        Some(&AuxRef { x: Some(rx), y: ry }),
    )
    .unwrap();
    assert_eq!(computed.columns.len(), 1);
    assert_eq!(computed.columns[0].key, "corrected");
    assert_eq!(computed.columns[0].data, vec![5.0, 10.0, 15.0]);
    assert_eq!(computed.results.len(), 1);
}

#[test]
fn compute_nomme_la_colonne_d_entree_disparue() {
    let mut f = fixture();
    f.project.remove(f.y1).unwrap();
    let err = compute(
        f.project.tree(),
        &Operation::Correct {
            op: CorrectOp::Divide,
        },
        &[SeriesRef {
            x: Some(f.x),
            y: f.y1,
            sigma: None,
        }],
        None,
    )
    .unwrap_err();
    assert_eq!(err, ComputeError::MissingInput(f.y1));
}

#[test]
fn refresh_dit_s_il_a_ecrit_quelque_chose() {
    let mut f = fixture();
    let id = f
        .project
        .add_calculation(draft(
            &f,
            vec![out(0, "corrected", vec![1.0, 2.0, 3.0], "C")],
        ))
        .unwrap();
    let same = f
        .project
        .refresh_calculation(
            id,
            &[Some(vec![1.0, 2.0, 3.0])],
            vec![SeriesResult::default(), SeriesResult::default()],
        )
        .unwrap();
    assert!(!same, "mêmes données, rien d'écrit");
    let diff = f
        .project
        .refresh_calculation(
            id,
            &[Some(vec![1.0, 2.0, 4.0])],
            vec![SeriesResult::default(), SeriesResult::default()],
        )
        .unwrap();
    assert!(diff);
}

/// Un graphe qui trace la colonne de sortie d'un calcul et la colonne source.
fn graph_des_deux(f: &mut Fixture, stale_warning: bool) -> (NodeId, NodeId) {
    let id = f
        .project
        .add_calculation(draft(
            f,
            vec![out(0, "corrected", vec![1.0, 2.0, 3.0], "Y1 (corr.)")],
        ))
        .unwrap();
    let produced = f.project.tree().calculation(id).unwrap().outputs[0]
        .column
        .unwrap();
    let mut spec = stokesforge_plot::PlotSpec {
        stale_warning,
        ..Default::default()
    };
    let panel = spec.panels.get_mut(0).unwrap();
    panel
        .series
        .push(stokesforge_plot::SeriesSpec::new(produced));
    panel.series.push(stokesforge_plot::SeriesSpec::new(f.y2));
    f.project.add_graph(f.project.root(), None, spec).unwrap();
    (id, produced)
}

fn drapeaux(f: &Fixture) -> Vec<bool> {
    let graph = f.project.tree().children(f.project.root()).unwrap()[1];
    let stokesforge_core::NodeKind::Graph { spec, .. } =
        f.project.tree().node(graph).unwrap().kind()
    else {
        panic!("un graphe attendu");
    };
    stokesforge_core::resolve_spec(f.project.tree(), spec).panels[0]
        .series
        .iter()
        .map(|s| s.stale)
        .collect()
}

#[test]
fn une_serie_tracee_depuis_un_calcul_perime_est_marquee_et_le_graphe_peut_y_renoncer() {
    let mut f = fixture();
    let (id, _) = graph_des_deux(&mut f, true);
    assert_eq!(drapeaux(&f), [false, false], "calcul à jour : rien");
    f.project
        .set_calculation_stale(id, Some(Staleness::Manual))
        .unwrap();
    assert_eq!(drapeaux(&f), [true, false], "seule la sortie est marquée");
    f.project.refresh_calculation(id, &[None], vec![]).unwrap();
    assert_eq!(drapeaux(&f), [false, false], "recalculé : plus de marque");

    // Le graphe qui a désactivé l'avertissement ne marque plus rien.
    let mut g = fixture();
    let (id, _) = graph_des_deux(&mut g, false);
    g.project
        .set_calculation_stale(id, Some(Staleness::Manual))
        .unwrap();
    assert_eq!(drapeaux(&g), [false, false]);
}

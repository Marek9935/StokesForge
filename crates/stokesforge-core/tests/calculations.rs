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
        low_r2_dialog: true,
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

/// Une opération à valeur non finie ne s'enregistre pas (#278) : elle serait
/// écrite puis perdue à la relecture du `.stf`.
#[test]
fn une_operation_non_finie_est_refusee_a_l_ajout_et_a_la_mise_a_jour() {
    use stokesforge_analysis::baseline::{BaselineMethod, BaselineOp};
    let nan_baseline = || Operation::Baseline {
        method: BaselineMethod::Constant {
            point: (f64::NAN, 1.0),
        },
        op: BaselineOp::Subtract,
        keep_baseline: false,
    };
    let mut f = fixture();
    let before = f.project.tree().clone();
    let mut d = draft(&f, vec![out(0, "corrected", vec![1.0], "a")]);
    d.operation = nan_baseline();
    assert!(matches!(
        f.project.add_calculation(d),
        Err(CoreError::Structure(_))
    ));
    assert_eq!(f.project.tree(), &before);

    let id = f
        .project
        .add_calculation(draft(&f, vec![out(0, "corrected", vec![1.0], "a")]))
        .unwrap();
    let before = f.project.tree().clone();
    let mut d = draft(&f, vec![out(0, "corrected", vec![1.0], "a")]);
    d.operation = nan_baseline();
    assert!(matches!(
        f.project.update_calculation(id, d),
        Err(CoreError::Structure(_))
    ));
    assert_eq!(f.project.tree(), &before);
}

/// Une opération non finie par famille et par champ : `(nom attendu dans
/// l'erreur, opération)`.
fn operations_non_finies() -> Vec<(&'static str, Operation)> {
    use stokesforge_analysis::baseline::{BaselineMethod, BaselineOp};
    use stokesforge_analysis::deconv::{DirectMethod, ReconvSpec};
    use stokesforge_analysis::fit::FitSpec;
    use stokesforge_analysis::integrate::{IntegrateSpec, Under};
    use stokesforge_analysis::model::Model;
    use stokesforge_analysis::ops::AutoDetect;
    use stokesforge_analysis::peaks::{PeakBackground, PeakSeed, PeakShape, PeaksSpec};
    let base = |method| Operation::Baseline {
        method,
        op: BaselineOp::Subtract,
        keep_baseline: false,
    };
    let peaks = |seeds: Vec<PeakSeed>, range, prominence| Operation::Peaks {
        spec: PeaksSpec {
            shape: PeakShape::Gaussian,
            background: PeakBackground::Constant,
            seeds,
            link_widths: false,
            center_window: None,
        },
        auto: Some(AutoDetect {
            min_prominence: prominence,
            max_peaks: 3,
        }),
        range,
        residuals: false,
    };
    let nan = f64::NAN;
    let inf = f64::INFINITY;
    vec![
        (
            "point",
            base(BaselineMethod::Constant { point: (nan, 1.0) }),
        ),
        (
            "zone",
            base(BaselineMethod::ConstantZone { zone: (1.0, inf) }),
        ),
        (
            "points",
            base(BaselineMethod::Line {
                p1: (0.0, 1.0),
                p2: (nan, 2.0),
            }),
        ),
        // Les ancres non posées sont ignorées, pas refusées (#298).
        (
            "zones",
            base(BaselineMethod::Zones {
                zones: vec![(0.0, 1.0), (nan, 3.0)],
                degree: 1,
            }),
        ),
        (
            "range",
            Operation::Fit {
                spec: FitSpec {
                    model: Model::Linear,
                    params: Vec::new(),
                },
                range: Some((0.0, nan)),
                residuals: false,
            },
        ),
        (
            "ranges",
            Operation::Integrate {
                spec: IntegrateSpec {
                    ranges: vec![(nan, 1.0)],
                    under: Under::Zero,
                    cumulative: false,
                    normalize: false,
                },
            },
        ),
        (
            "seeds",
            peaks(
                vec![PeakSeed {
                    center: nan,
                    height: None,
                    fwhm: None,
                }],
                None,
                0.1,
            ),
        ),
        ("range", peaks(Vec::new(), Some((inf, 1.0)), 0.1)),
        ("min_prominence", peaks(Vec::new(), None, nan)),
        (
            "window",
            Operation::Reconvolution {
                spec: ReconvSpec {
                    components: 1,
                    params: Vec::new(),
                    shift_initial: 0.0,
                    fit_shift: false,
                    window: Some((nan, 1.0)),
                },
                residuals: false,
            },
        ),
        (
            "shift",
            Operation::DirectDeconvolution {
                method: DirectMethod::RichardsonLucy { iterations: 5 },
                shift: nan,
            },
        ),
    ]
}

/// Chaque famille et chaque champ non fini (#278) : `add_calculation`,
/// `update_calculation` et `replace_analysis_data` refusent par une
/// `Structure` qui nomme le champ, sans toucher l'arbre ni consommer
/// d'identifiant.
#[test]
fn toute_operation_non_finie_est_refusee_sans_toucher_l_arbre() {
    let mut f = fixture();
    let id = f
        .project
        .add_calculation(draft(&f, vec![out(0, "corrected", vec![1.0], "a")]))
        .unwrap();
    let versions: Vec<(NodeId, u64)> = [f.x, f.y1, f.y2]
        .into_iter()
        .map(|c| {
            let v = f
                .project
                .tree()
                .get(c)
                .unwrap()
                .as_column()
                .unwrap()
                .version();
            (c, v)
        })
        .collect();
    let refuse = |e: CoreError, nom: &str| match e {
        CoreError::Structure(message) => {
            assert!(message.contains(&format!("'{nom}'")), "{message}")
        }
        other => panic!("attendu Structure, reçu {other:?}"),
    };
    for (nom, operation) in operations_non_finies() {
        let before = f.project.tree().clone();
        let next = f.project.next_id_hint();
        let mut d = draft(&f, vec![out(0, "corrected", vec![1.0], "a")]);
        d.operation = operation.clone();
        refuse(f.project.add_calculation(d.clone()).unwrap_err(), nom);
        refuse(
            f.project.update_calculation(id, d.clone()).unwrap_err(),
            nom,
        );
        // La remise à la place des données valide d'abord l'opération : même
        // refus, avant toute vérification de version ou d'entrée.
        refuse(
            f.project.replace_analysis_data(d, &versions).unwrap_err(),
            nom,
        );
        assert_eq!(f.project.tree(), &before, "{nom}");
        assert_eq!(f.project.next_id_hint(), next, "{nom} : aucun id consommé");
    }
}

/// Le contrôle de finitude n'écarte pas l'ordinaire : une opération finie
/// s'ajoute, se met à jour, et la valeur extrême finie passe.
#[test]
fn une_operation_finie_s_ajoute_et_se_met_a_jour() {
    use stokesforge_analysis::baseline::{BaselineMethod, BaselineOp};
    let mut f = fixture();
    let constant = |v: f64| Operation::Baseline {
        method: BaselineMethod::Constant { point: (v, -v) },
        op: BaselineOp::Subtract,
        keep_baseline: false,
    };
    let mut d = draft(&f, vec![out(0, "corrected", vec![1.0], "a")]);
    d.operation = constant(f64::MAX);
    let id = f.project.add_calculation(d.clone()).unwrap();
    d.operation = constant(0.5);
    f.project.update_calculation(id, d).unwrap();
    assert_eq!(
        f.project.tree().calculation(id).unwrap().operation,
        constant(0.5)
    );
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

// ── Colonnes de longueurs différentes (C4, #313) ───────────────────────

/// Un classeur à X, Y de longueurs données, et une référence constante = 2 sur
/// un large domaine : `Correct`/`Divide` rend y / 2.
fn inegales(x: &[f64], y: &[f64]) -> (Project, NodeId, SeriesRef, AuxRef) {
    let mut project = Project::new("P").unwrap();
    let book = project.add_workbook(project.root(), Some("Book")).unwrap();
    let sheet = project.tree().children(book).unwrap()[0];
    let mut add = |name: &str, v: &[f64]| {
        project
            .add_column(sheet, None, Some(name), Column::from_f64(v))
            .unwrap()
    };
    let cx = add("X", x);
    let cy = add("Y", y);
    let rx = add("RX", &[-100.0, 100.0]);
    let ry = add("RY", &[2.0, 2.0]);
    (
        project,
        cy,
        SeriesRef {
            x: Some(cx),
            y: cy,
            sigma: None,
        },
        AuxRef { x: Some(rx), y: ry },
    )
}

fn corrige(project: &Project, series: SeriesRef, aux: &AuxRef) -> Vec<f64> {
    let computed = compute(
        project.tree(),
        &Operation::Correct {
            op: CorrectOp::Divide,
        },
        &[series],
        Some(aux),
    )
    .unwrap();
    computed.columns[0].data.clone()
}

#[test]
fn la_sortie_a_la_longueur_de_x_quand_x_est_donne() {
    // X plus court que Y : les lignes de Y au-delà de X n'ont pas d'abscisse.
    let (p, _, s, aux) = inegales(&[1.0, 2.0, 3.0], &[2.0, 4.0, 6.0, 8.0, 10.0]);
    assert_eq!(corrige(&p, s, &aux), vec![1.0, 2.0, 3.0]);
    // X plus long que Y : les lignes de Y manquantes sont vides (NaN).
    let (p, _, s, aux) = inegales(&[1.0, 2.0, 3.0, 4.0, 5.0], &[2.0, 4.0, 6.0]);
    let data = corrige(&p, s, &aux);
    assert_eq!(data.len(), 5);
    assert_eq!(&data[..3], &[1.0, 2.0, 3.0]);
    assert!(data[3].is_nan() && data[4].is_nan());
}

#[test]
fn sans_x_la_sortie_a_la_longueur_de_y() {
    let (p, _, mut s, aux) = inegales(&[1.0, 2.0], &[2.0, 4.0, 6.0, 8.0]);
    s.x = None;
    assert_eq!(corrige(&p, s, &aux), vec![1.0, 2.0, 3.0, 4.0]);
}

#[test]
fn le_remplacement_avec_y_plus_long_que_x_garde_les_cellules_au_dela_de_x() {
    let (mut p, y, s, aux) = inegales(&[1.0, 2.0, 3.0], &[2.0, 4.0, 6.0, 8.0, 10.0]);
    let data = corrige(&p, s.clone(), &aux);
    let versions: Vec<(NodeId, u64)> = [s.x.unwrap(), y, aux.y, aux.x.unwrap()]
        .into_iter()
        .map(|c| (c, p.tree().get(c).unwrap().as_column().unwrap().version()))
        .collect();
    let mut draft = CalculationDraft {
        operation: Operation::Correct {
            op: CorrectOp::Divide,
        },
        series: vec![s],
        aux: Some(aux),
        outputs: vec![out(0, "corrected", data, "c")],
        results: vec![SeriesResult::default()],
        keep_results: false,
        auto_recalc: false,
        low_r2_dialog: false,
    };
    // Sans complétion, la garde de longueur du cœur refuse la sortie courte.
    assert!(matches!(
        p.replace_analysis_data(draft.clone(), &versions),
        Err(CoreError::Structure(_))
    ));
    draft.complete_for_replacement(p.tree());
    assert_eq!(draft.outputs[0].data.len(), 5);
    p.replace_analysis_data(draft, &versions).unwrap();
    let col = p.tree().get(y).unwrap().as_column().unwrap();
    let got: Vec<f64> = (0..5).map(|r| col.values().number_at(r)).collect();
    // Les trois premières lignes sont corrigées, les deux autres gardent l'original.
    assert_eq!(got, vec![1.0, 2.0, 3.0, 8.0, 10.0]);
}

/// #324 : un lissage remplace les Y par sa sortie `smoothed` (comme une
/// correction par `corrected`) ; une sortie `corrected` ne lui convient pas.
#[test]
fn un_lissage_remplace_les_y_par_sa_sortie_lissee() {
    use stokesforge_analysis::smooth::{SmoothMethod, SmoothSpec, SmoothWindow};
    let mut f = fixture();
    let versions: Vec<(NodeId, u64)> = [f.x, f.y1]
        .into_iter()
        .map(|c| {
            (
                c,
                f.project
                    .tree()
                    .get(c)
                    .unwrap()
                    .as_column()
                    .unwrap()
                    .version(),
            )
        })
        .collect();
    let smooth = |key: &str| CalculationDraft {
        operation: Operation::Smooth {
            spec: SmoothSpec {
                method: SmoothMethod::AdjacentAverage,
                window: SmoothWindow::Points(3),
            },
        },
        series: vec![SeriesRef {
            x: Some(f.x),
            y: f.y1,
            sigma: None,
        }],
        aux: None,
        outputs: vec![out(0, key, vec![12.0, 20.0, 28.0], "Y1 (smoothed)")],
        results: vec![SeriesResult::default()],
        keep_results: false,
        auto_recalc: false,
        low_r2_dialog: false,
    };
    assert!(matches!(
        f.project
            .replace_analysis_data(smooth("corrected"), &versions),
        Err(CoreError::Structure(_))
    ));
    f.project
        .replace_analysis_data(smooth("smoothed"), &versions)
        .unwrap();
    let column = f.project.tree().get(f.y1).unwrap().as_column().unwrap();
    let got: Vec<f64> = (0..3).map(|r| column.values().number_at(r)).collect();
    assert_eq!(got, vec![12.0, 20.0, 28.0]);
}

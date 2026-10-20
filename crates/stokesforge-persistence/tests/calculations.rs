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
                warnings: vec![
                    Warning::NegativeClipped(4),
                    Warning::IrfStepMismatch,
                    Warning::Undetermined(2),
                    Warning::NotConverged,
                    Warning::SignalStepNotUniform,
                    Warning::Undetermined(17),
                    Warning::IrfTruncated,
                    Warning::BridgedReferenceGap(3),
                    Warning::DivisionByZero(2),
                ],
            }],
            keep_results: true,
            auto_recalc: false,
            low_r2_dialog: false,
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
fn le_reglage_de_boite_r2_faible_d_un_calcul_survit_au_fichier() {
    // La fixture l'éteint (#288) : le défaut `true` ne doit pas le rallumer.
    let project = project_with_calculation();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("r2.stf");
    save(&project, &path).unwrap();
    let back = load(&path).unwrap();
    let calc = back.tree().calculations().values().next().unwrap();
    assert!(!calc.low_r2_dialog);
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

#[test]
fn les_deux_lignes_de_base_constantes_traversent_le_fichier_stf() {
    use stokesforge_analysis::baseline::{BaselineMethod, BaselineOp};
    for method in [
        BaselineMethod::Constant { point: (2.5, 7.0) },
        BaselineMethod::ConstantZone { zone: (1.0, 3.0) },
    ] {
        let mut project = project_with_calculation();
        let old = project
            .tree()
            .calculations()
            .values()
            .next()
            .unwrap()
            .clone();
        let operation = Operation::Baseline {
            method,
            op: BaselineOp::Subtract,
            keep_baseline: false,
        };
        project
            .add_calculation(CalculationDraft {
                operation: operation.clone(),
                series: old.series.clone(),
                aux: None,
                outputs: Vec::new(),
                results: Vec::new(),
                keep_results: false,
                auto_recalc: true,
                low_r2_dialog: true,
            })
            .unwrap();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("constant.stf");
        save(&project, &path).unwrap();
        let back = load(&path).unwrap();
        assert!(back
            .tree()
            .calculations()
            .values()
            .any(|c| c.operation == operation));
        assert_eq!(back.tree(), project.tree());
    }
}

#[test]
fn les_corrections_definitives_survivent_au_fichier_sans_lien_de_calcul() {
    let mut project = project_with_calculation();
    let (id, calc) = project
        .tree()
        .calculations()
        .iter()
        .next()
        .map(|(id, c)| (*id, c.clone()))
        .unwrap();
    let output = calc.outputs[0].column.unwrap();
    project.remove_calculation(id, true).unwrap();
    let y = calc.series[0].y;
    for correction in [1., 2.] {
        let versions = calc
            .series
            .iter()
            .flat_map(|s| [Some(s.y), s.x].into_iter().flatten())
            .map(|id| {
                (
                    id,
                    project
                        .tree()
                        .get(id)
                        .unwrap()
                        .as_column()
                        .unwrap()
                        .version(),
                )
            })
            .collect::<Vec<_>>();
        project
            .replace_analysis_data(
                CalculationDraft {
                    operation: calc.operation.clone(),
                    series: calc.series.clone(),
                    aux: None,
                    outputs: vec![OutputDraft {
                        series: 0,
                        key: "corrected".into(),
                        index: None,
                        meta: ColumnMeta::default(),
                        data: vec![correction; 3],
                    }],
                    results: calc.results.clone(),
                    keep_results: false,
                    auto_recalc: false,
                    low_r2_dialog: true,
                },
                &versions,
            )
            .unwrap();
    }
    assert!(project.tree().node(output).is_none());
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("corrections.stf");
    save(&project, &path).unwrap();
    let back = load(&path).unwrap();
    assert_eq!(back.tree(), project.tree());
    assert_eq!(back.tree().correction_traces().len(), 2);
    // #308 : les deux corrections de Y partagent la même donnée brute, avant
    // comme après le fichier.
    let copies: Vec<_> = back
        .tree()
        .correction_traces()
        .values()
        .map(|t| t.originals[0].1)
        .collect();
    assert_eq!(copies[0], copies[1]);
    assert!(back.tree().calculations().is_empty());
    assert!(back.tree().calculation_of_column(y).is_none());
    let mut back = back;
    let next = back.add_note(back.root(), None, "suivant").unwrap();
    assert!(back
        .tree()
        .correction_traces()
        .keys()
        .all(|id| id.raw() < next.raw()));
}

/// Une correction par une référence figée, remplaçant les données (#248) : le
/// classeur caché, la copie de l'original et la référence survivent au fichier,
/// et « Restore original data » marche après rechargement.
#[test]
fn les_copies_figees_et_les_originaux_survivent_au_fichier() {
    use stokesforge_core::AuxRef;
    let mut project = Project::new("P").unwrap();
    let book = project.add_workbook(project.root(), Some("Book")).unwrap();
    let sheet = project.tree().children(book).unwrap()[0];
    let mut add = |name: &str, values: &[f64]| {
        project
            .add_column(sheet, None, Some(name), Column::from_f64(values))
            .unwrap()
    };
    let x = add("X", &[1.0, 2.0, 3.0]);
    let y = add("Y", &[10.0, 20.0, 30.0]);
    let rx = add("RX", &[1.0, 2.0, 3.0]);
    let ry = add("RY", &[2.0, 2.0, 2.0]);
    let versions: Vec<_> = [x, y, rx, ry]
        .iter()
        .map(|id| {
            let version = project
                .tree()
                .get(*id)
                .unwrap()
                .as_column()
                .unwrap()
                .version();
            (*id, version)
        })
        .collect();
    project
        .replace_analysis_data(
            CalculationDraft {
                operation: Operation::Correct {
                    op: CorrectOp::Divide,
                },
                series: vec![SeriesRef {
                    x: Some(x),
                    y,
                    sigma: None,
                }],
                aux: Some(AuxRef { x: Some(rx), y: ry }),
                outputs: vec![OutputDraft {
                    series: 0,
                    key: "corrected".into(),
                    index: None,
                    meta: ColumnMeta::default(),
                    data: vec![5.0, 10.0, 15.0],
                }],
                results: vec![SeriesResult::default()],
                keep_results: false,
                auto_recalc: false,
                low_r2_dialog: true,
            },
            &versions,
        )
        .unwrap();
    assert!(project.analysis_store_sheet().is_some());

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("copies.stf");
    save(&project, &path).unwrap();
    let mut back = load(&path).unwrap();
    assert_eq!(back.tree(), project.tree());
    assert!(
        back.analysis_store_sheet().is_some(),
        "le classeur caché revient marqué"
    );
    let trace = *back.tree().correction_traces().keys().next().unwrap();
    assert_eq!(back.tree().correction_traces()[&trace].originals.len(), 1);
    back.restore_original_data(trace).unwrap();
    let restored: Vec<f64> = back
        .tree()
        .get(y)
        .unwrap()
        .as_column()
        .unwrap()
        .numeric_view()
        .unwrap()
        .to_vec();
    assert_eq!(restored, vec![10.0, 20.0, 30.0]);
    assert!(back.analysis_store_sheet().is_none());
}

/// Un calcul dont l'opération est illisible n'est pas perdu en silence (#278) :
/// le rapport de chargement le nomme, et le reste du projet se charge.
#[test]
fn un_calcul_illisible_est_nomme_par_le_rapport_et_le_reste_se_charge() {
    let project = project_with_calculation();
    let id = project.tree().calculations().keys().next().unwrap().raw();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("p.stf");
    save(&project, &path).unwrap();

    // Même longueur : on abîme l'opération (« subtract » n'existe plus) sans
    // décaler le moindre octet du fichier.
    let mut bytes = std::fs::read(&path).unwrap();
    let needle = b"subtract";
    let at = bytes
        .windows(needle.len())
        .position(|w| w == needle)
        .expect("l'opération est écrite dans le fichier");
    bytes[at..at + needle.len()].copy_from_slice(b"subtraXX");
    std::fs::write(&path, &bytes).unwrap();

    let (loaded, _geometries, report) = stokesforge_persistence::load_with_report(&path).unwrap();
    assert!(loaded.tree().calculations().is_empty());
    assert_eq!(
        report.discarded_calculations,
        vec![stokesforge_persistence::DiscardedCalculation {
            id,
            kind: Some("correct".into())
        }]
    );
    // Le reste du projet est là.
    assert_eq!(
        loaded.tree().root(),
        project.tree().root(),
        "la racine est celle du fichier"
    );
    assert_eq!(loaded.tree().len(), project.tree().len());
    // Un fichier sain ne rapporte rien.
    save(&project, &path).unwrap();
    assert!(stokesforge_persistence::load_with_report(&path)
        .unwrap()
        .2
        .is_clean());
}

// ── Calculs écartés au chargement (#278) ───────────────────────────────────

/// Un projet à trois calculs de correction sur trois colonnes Y distinctes :
/// `(projet, [(identifiant du calcul, mot d'opération écrit dans le fichier)])`
/// dans l'ordre croissant des identifiants.
fn project_with_three_calculations() -> (Project, Vec<(u64, &'static str)>) {
    let mut project = Project::new("P").unwrap();
    let book = project.add_workbook(project.root(), Some("Book")).unwrap();
    let sheet = project.tree().children(book).unwrap()[0];
    let mut made = Vec::new();
    for (name, op, word) in [
        ("Y1", CorrectOp::Subtract, "subtract"),
        ("Y2", CorrectOp::Divide, "divide"),
        ("Y3", CorrectOp::Multiply, "multiply"),
    ] {
        let y = project
            .add_column(sheet, None, Some(name), Column::from_f64(&[1.0, 2.0]))
            .unwrap();
        let id = project
            .add_calculation(CalculationDraft {
                operation: Operation::Correct { op },
                series: vec![SeriesRef {
                    x: None,
                    y,
                    sigma: None,
                }],
                aux: None,
                outputs: vec![OutputDraft {
                    series: 0,
                    key: "corrected".into(),
                    index: None,
                    meta: ColumnMeta::default(),
                    data: vec![3.0, 4.0],
                }],
                results: vec![SeriesResult::default()],
                keep_results: true,
                auto_recalc: false,
                low_r2_dialog: false,
            })
            .unwrap();
        made.push((id.raw(), word));
    }
    (project, made)
}

/// Remplace, dans les octets du fichier, `from` par `to` (même longueur).
fn patch(path: &std::path::Path, from: &[u8], to: &[u8]) {
    assert_eq!(from.len(), to.len());
    let mut bytes = std::fs::read(path).unwrap();
    let at = bytes
        .windows(from.len())
        .position(|w| w == from)
        .expect("le motif est dans le fichier");
    bytes[at..at + to.len()].copy_from_slice(to);
    std::fs::write(path, bytes).unwrap();
}

#[test]
fn plusieurs_calculs_ecartes_sont_tous_listes_dans_l_ordre_et_les_sains_se_chargent() {
    let (project, made) = project_with_three_calculations();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("p.stf");
    save(&project, &path).unwrap();
    patch(&path, b"subtract", b"subtraXX");
    patch(&path, b"multiply", b"multipXX");

    let expected = vec![
        stokesforge_persistence::DiscardedCalculation {
            id: made[0].0,
            kind: Some("correct".into()),
        },
        stokesforge_persistence::DiscardedCalculation {
            id: made[2].0,
            kind: Some("correct".into()),
        },
    ];
    // Même rapport à chaque lecture : l'ordre ne dépend de rien d'aléatoire.
    for _ in 0..3 {
        let (loaded, _, report) = stokesforge_persistence::load_with_report(&path).unwrap();
        assert_eq!(report.discarded_calculations, expected);
        assert!(!report.is_clean());
        let kept: Vec<u64> = loaded
            .tree()
            .calculations()
            .keys()
            .map(|k| k.raw())
            .collect();
        assert_eq!(kept, vec![made[1].0], "seul le sain est gardé");
    }
    // `load` et `load_with_geometries` gardent leur comportement : ils
    // chargent le même projet, sans rapport et sans échec.
    let by_load = load(&path).unwrap();
    let (by_geometries, _) = stokesforge_persistence::load_with_geometries(&path).unwrap();
    let (by_report, _, _) = stokesforge_persistence::load_with_report(&path).unwrap();
    assert_eq!(by_load.tree(), by_report.tree());
    assert_eq!(by_geometries.tree(), by_report.tree());
    // Les colonnes de sortie d'un calcul écarté restent des colonnes ordinaires.
    assert_eq!(by_report.tree().len(), project.tree().len());
}

#[test]
fn un_fichier_sans_calcul_ecarte_rend_un_rapport_vide() {
    let (project, made) = project_with_three_calculations();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("p.stf");
    save(&project, &path).unwrap();
    let (loaded, _, report) = stokesforge_persistence::load_with_report(&path).unwrap();
    assert!(report.is_clean());
    assert_eq!(report, stokesforge_persistence::LoadReport::default());
    assert_eq!(loaded.tree().calculations().len(), made.len());
    // Un projet sans aucun calcul aussi.
    let empty = Project::new("Vide").unwrap();
    save(&empty, &path).unwrap();
    assert!(stokesforge_persistence::load_with_report(&path)
        .unwrap()
        .2
        .is_clean());
}

/// Un NaN écrit à la main dans un point de ligne de base ne doit pas entrer
/// dans le modèle (il serait perdu à la resauvegarde) : le calcul est écarté
/// **et** signalé, comme n'importe quel enregistrement illisible.
#[test]
fn un_nan_edite_a_la_main_dans_le_fichier_ecarte_et_signale_le_calcul() {
    use stokesforge_analysis::baseline::{BaselineMethod, BaselineOp};
    let mut project = Project::new("P").unwrap();
    let book = project.add_workbook(project.root(), Some("Book")).unwrap();
    let sheet = project.tree().children(book).unwrap()[0];
    let y = project
        .add_column(sheet, None, Some("Y"), Column::from_f64(&[1.0, 2.0]))
        .unwrap();
    let id = project
        .add_calculation(CalculationDraft {
            operation: Operation::Baseline {
                method: BaselineMethod::Constant {
                    point: (1234.5, 6.25),
                },
                op: BaselineOp::Subtract,
                keep_baseline: false,
            },
            series: vec![SeriesRef {
                x: None,
                y,
                sigma: None,
            }],
            aux: None,
            outputs: vec![OutputDraft {
                series: 0,
                key: "corrected".into(),
                index: None,
                meta: ColumnMeta::default(),
                data: vec![0.0, 1.0],
            }],
            results: vec![SeriesResult::default()],
            keep_results: true,
            auto_recalc: false,
            low_r2_dialog: false,
        })
        .unwrap()
        .raw();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("p.stf");
    save(&project, &path).unwrap();
    let mut from = vec![0xcb];
    from.extend_from_slice(&1234.5_f64.to_be_bytes());
    let mut to = vec![0xcb];
    to.extend_from_slice(&f64::NAN.to_be_bytes());
    patch(&path, &from, &to);

    let (loaded, _, report) = stokesforge_persistence::load_with_report(&path).unwrap();
    assert!(
        loaded.tree().calculations().is_empty(),
        "un NaN ne doit pas entrer dans le modèle"
    );
    assert_eq!(
        report.discarded_calculations,
        vec![stokesforge_persistence::DiscardedCalculation {
            id,
            kind: Some("baseline".into())
        }]
    );
}

/// #309 : une trace de correction illisible n'est pas perdue sans bruit : le
/// rapport la compte (`discarded_corrections`), le reste se charge.
#[test]
fn une_trace_de_correction_illisible_est_comptee_par_le_rapport() {
    let mut project = Project::new("P").unwrap();
    let book = project.add_workbook(project.root(), Some("Book")).unwrap();
    let sheet = project.tree().children(book).unwrap()[0];
    let y = project
        .add_column(sheet, None, Some("Y"), Column::from_f64(&[10.0, 20.0]))
        .unwrap();
    let version = project
        .tree()
        .get(y)
        .unwrap()
        .as_column()
        .unwrap()
        .version();
    let trace = project
        .replace_analysis_data(
            CalculationDraft {
                operation: Operation::Correct {
                    op: CorrectOp::Subtract,
                },
                series: vec![SeriesRef {
                    x: None,
                    y,
                    sigma: None,
                }],
                aux: None,
                outputs: vec![OutputDraft {
                    series: 0,
                    key: "corrected".into(),
                    index: None,
                    meta: ColumnMeta::default(),
                    data: vec![0.0, 10.0],
                }],
                results: vec![SeriesResult::default()],
                keep_results: false,
                auto_recalc: false,
                low_r2_dialog: true,
            },
            &[(y, version)],
        )
        .unwrap()
        .raw();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("p.stf");
    save(&project, &path).unwrap();
    let clean = stokesforge_persistence::load_with_report(&path).unwrap();
    assert!(clean.2.is_clean());
    assert_eq!(clean.0.tree().correction_traces().len(), 1);

    patch(&path, b"subtract", b"subtraXX");
    let (loaded, _, report) = stokesforge_persistence::load_with_report(&path).unwrap();
    assert!(loaded.tree().correction_traces().is_empty());
    assert!(report.discarded_calculations.is_empty());
    assert_eq!(
        report.discarded_corrections,
        vec![stokesforge_persistence::DiscardedCalculation {
            id: trace,
            kind: Some("correct".into())
        }]
    );
    assert!(!report.is_clean());
}

/// A1 (#316) : l'empreinte du contenu des entrées, écrite à l'enregistrement,
/// est comparée à la lecture sans rien recalculer.
#[test]
fn des_entrees_modifiees_dans_le_fichier_perimment_le_calcul_sans_le_recalculer() {
    let (project, id) = {
        let project = project_with_calculation();
        let id = *project.tree().calculations().keys().next().unwrap();
        (project, id)
    };
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("p.stf");
    save(&project, &path).unwrap();

    // Intact : rien de périmé.
    let loaded = load(&path).unwrap();
    assert_eq!(loaded.tree().calculation(id).unwrap().stale, None);

    // Y devient [10, 20, 31] hors StokesForge (même longueur, aucun décalage).
    let octets = |v: &[f64]| -> Vec<u8> { v.iter().flat_map(|x| x.to_le_bytes()).collect() };
    let (avant, apres) = (octets(&[10.0, 20.0, 30.0]), octets(&[10.0, 20.0, 31.0]));
    let mut bytes = std::fs::read(&path).unwrap();
    let at = bytes
        .windows(avant.len())
        .position(|w| w == avant.as_slice())
        .expect("Y est dans le fichier");
    bytes[at..at + avant.len()].copy_from_slice(&apres);
    std::fs::write(&path, &bytes).unwrap();

    let loaded = load(&path).unwrap();
    let calc = loaded.tree().calculation(id).unwrap();
    assert_eq!(calc.stale, Some(Staleness::Manual));
    // Les valeurs de sortie sont celles du fichier : rien n'a été recalculé.
    let out = calc.outputs[0].column.unwrap();
    let values = loaded
        .tree()
        .node(out)
        .unwrap()
        .as_column()
        .unwrap()
        .numeric_view()
        .unwrap()
        .to_vec();
    assert_eq!(values[..2], [9.0, 18.0]);
}

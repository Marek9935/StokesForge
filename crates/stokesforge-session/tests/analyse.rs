//! Recalcul des calculs d'analyse (#181), vu depuis l'API publique de la
//! session : un calcul automatique suit ses entrées, un calcul manuel se
//! marque périmé sans bouger, un échec garde les dernières valeurs, une entrée
//! perdue puis retrouvée repart de zéro.
//!
//! Le contrat « le recalcul ciblé donne exactement le recalcul complet » est
//! tenu par la batterie différentielle `stokesforge-robustesse` (`b22`) ; ces
//! cas nomment chaque comportement.

use stokesforge_analysis::fit::FitSpec;
use stokesforge_analysis::model::Model;
use stokesforge_analysis::ops::Operation;
use stokesforge_commands::Command;
use stokesforge_core::{
    compute, CalculationDraft, Column, ColumnMeta, NodeId, OutputDraft, Project, SeriesRef,
    Staleness, ValueBlock,
};
use stokesforge_session::{FormulaRecalc, Session, SessionHandle, Snapshot};

struct Montage {
    sheet: NodeId,
    x: NodeId,
    y: NodeId,
}

fn demarrer(mode: FormulaRecalc) -> (Session, SessionHandle, Montage) {
    let mut project = Project::new("Demo").unwrap();
    let root = project.root();
    project.add_workbook(root, Some("Book1")).unwrap();
    let sheet = project.resolve_path("[Book1]Sheet1").unwrap();
    let x = project
        .add_column(
            sheet,
            None,
            Some("X"),
            Column::from_f64(&[1.0, 2.0, 3.0, 4.0]),
        )
        .unwrap();
    let y = project
        .add_column(
            sheet,
            None,
            Some("Y"),
            Column::from_f64(&[10.0, 20.0, 30.0, 40.0]),
        )
        .unwrap();
    let session = Session::spawn_with_recalc(project, None, mode).unwrap();
    let handle = session.handle();
    (session, handle, Montage { sheet, x, y })
}

fn ajustement() -> Operation {
    Operation::Fit {
        spec: FitSpec {
            model: Model::Linear,
            params: vec![],
        },
        range: None,
        residuals: false,
    }
}

/// Ajoute un ajustement linéaire de (X, Y), automatique ou non.
fn ajouter(handle: &SessionHandle, m: &Montage, auto: bool) -> NodeId {
    let snapshot = handle.snapshot().unwrap();
    let series = vec![SeriesRef {
        x: Some(m.x),
        y: m.y,
        sigma: None,
    }];
    let op = ajustement();
    let computed = compute(&snapshot.tree, &op, &series, None).unwrap();
    let outputs = computed
        .columns
        .iter()
        .map(|c| OutputDraft {
            series: c.series,
            key: c.key.clone(),
            index: c.index,
            meta: ColumnMeta::default(),
            data: c.data.clone(),
        })
        .collect();
    handle
        .execute(Command::AddCalculation {
            draft: CalculationDraft {
                operation: op,
                series,
                aux: None,
                outputs,
                results: computed.results,
                keep_results: true,
                auto_recalc: auto,
            },
        })
        .unwrap();
    *handle
        .snapshot()
        .unwrap()
        .tree
        .calculations()
        .keys()
        .next()
        .unwrap()
}

fn saisir(column: NodeId, valeurs: &[f64]) -> Command {
    Command::SetCells {
        column,
        start: 0,
        values: ValueBlock::Float(valeurs.to_vec()),
        expected_version: None,
    }
}

fn pente(snapshot: &Snapshot, id: NodeId) -> f64 {
    snapshot.tree.calculation(id).unwrap().results[0]
        .values
        .iter()
        .find(|v| v.key == "param.slope")
        .unwrap()
        .value
}

fn colonne_fit(snapshot: &Snapshot, id: NodeId) -> Vec<f64> {
    let calc = snapshot.tree.calculation(id).unwrap();
    let out = calc.outputs.iter().find(|o| o.key == "fit").unwrap();
    snapshot
        .tree
        .node(out.column.unwrap())
        .unwrap()
        .as_column()
        .unwrap()
        .numeric_view()
        .unwrap()
        .to_vec()
}

fn modes() -> [FormulaRecalc; 2] {
    [FormulaRecalc::Targeted, FormulaRecalc::Full]
}

#[test]
fn un_calcul_automatique_suit_ses_entrees() {
    for mode in modes() {
        let (_session, handle, m) = demarrer(mode);
        let id = ajouter(&handle, &m, true);
        assert!((pente(&handle.snapshot().unwrap(), id) - 10.0).abs() < 1e-9);

        handle.execute(saisir(m.y, &[2.0, 4.0, 6.0, 8.0])).unwrap();
        let apres = handle.snapshot().unwrap();
        assert!((pente(&apres, id) - 2.0).abs() < 1e-9, "{mode:?}");
        assert!((colonne_fit(&apres, id)[3] - 8.0).abs() < 1e-9);
        assert_eq!(apres.tree.calculation(id).unwrap().stale, None);

        // L'annulation de la saisie ramène le calcul avec elle.
        handle.undo().unwrap();
        let annule = handle.snapshot().unwrap();
        assert!((pente(&annule, id) - 10.0).abs() < 1e-9, "{mode:?}");
    }
}

#[test]
fn le_recalcul_n_entre_pas_dans_l_historique() {
    let (_session, handle, m) = demarrer(FormulaRecalc::Targeted);
    let id = ajouter(&handle, &m, true);
    handle.execute(saisir(m.y, &[2.0, 4.0, 6.0, 8.0])).unwrap();
    handle.execute(saisir(m.y, &[3.0, 6.0, 9.0, 12.0])).unwrap();
    // Un seul « annuler » défait la dernière saisie — pas le recalcul qui l'a
    // suivie, qui n'a pas d'entrée à lui.
    handle.undo().unwrap();
    assert!((pente(&handle.snapshot().unwrap(), id) - 2.0).abs() < 1e-9);
}

#[test]
fn un_calcul_manuel_se_perime_sans_bouger_puis_se_relance() {
    for mode in modes() {
        let (_session, handle, m) = demarrer(mode);
        let id = ajouter(&handle, &m, false);
        let avant = handle.snapshot().unwrap();
        assert_eq!(avant.tree.calculation(id).unwrap().stale, None);

        handle.execute(saisir(m.y, &[2.0, 4.0, 6.0, 8.0])).unwrap();
        let perime = handle.snapshot().unwrap();
        assert_eq!(
            perime.tree.calculation(id).unwrap().stale,
            Some(Staleness::Manual),
            "{mode:?}"
        );
        assert!((pente(&perime, id) - 10.0).abs() < 1e-9, "valeurs gardées");
        assert_eq!(colonne_fit(&perime, id), colonne_fit(&avant, id));

        handle.recalculate(&[id]).unwrap();
        let frais = handle.snapshot().unwrap();
        assert_eq!(frais.tree.calculation(id).unwrap().stale, None);
        assert!((pente(&frais, id) - 2.0).abs() < 1e-9);
    }
}

#[test]
fn recalculer_sans_liste_relance_les_calculs_perimes() {
    let (_session, handle, m) = demarrer(FormulaRecalc::Targeted);
    let id = ajouter(&handle, &m, false);
    handle.execute(saisir(m.y, &[2.0, 4.0, 6.0, 8.0])).unwrap();
    handle.recalculate(&[]).unwrap();
    let snapshot = handle.snapshot().unwrap();
    assert_eq!(snapshot.tree.calculation(id).unwrap().stale, None);
    assert!((pente(&snapshot, id) - 2.0).abs() < 1e-9);
}

#[test]
fn modifier_la_definition_d_un_calcul_manuel_ne_le_perime_pas() {
    let (_session, handle, m) = demarrer(FormulaRecalc::Targeted);
    let id = ajouter(&handle, &m, false);
    // Annuler puis rétablir l'ajout redéfinit le calcul : aucune entrée n'a
    // bougé, il ne doit pas se déclarer périmé.
    handle.undo().unwrap();
    handle.redo().unwrap();
    let snapshot = handle.snapshot().unwrap();
    let id = *snapshot.tree.calculations().keys().next().unwrap_or(&id);
    assert_eq!(snapshot.tree.calculation(id).unwrap().stale, None);
}

#[test]
fn un_echec_garde_les_dernieres_valeurs_valides_puis_se_retablit() {
    for mode in modes() {
        let (_session, handle, m) = demarrer(mode);
        let id = ajouter(&handle, &m, true);
        let avant = handle.snapshot().unwrap();

        // Plus aucun point exploitable : l'ajustement ne peut pas se faire.
        handle
            .execute(saisir(m.y, &[f64::NAN, f64::NAN, f64::NAN, f64::NAN]))
            .unwrap();
        let echec = handle.snapshot().unwrap();
        assert!(
            matches!(
                echec.tree.calculation(id).unwrap().stale,
                Some(Staleness::Failed(_))
            ),
            "{mode:?}"
        );
        assert_eq!(colonne_fit(&echec, id), colonne_fit(&avant, id));
        assert!((pente(&echec, id) - 10.0).abs() < 1e-9);

        // Des données valides reviennent : le calcul se rétablit.
        handle.execute(saisir(m.y, &[2.0, 4.0, 6.0, 8.0])).unwrap();
        let repris = handle.snapshot().unwrap();
        assert_eq!(repris.tree.calculation(id).unwrap().stale, None);
        assert!((pente(&repris, id) - 2.0).abs() < 1e-9);
    }
}

#[test]
fn une_entree_supprimee_perime_le_calcul_et_l_annulation_le_rend() {
    for mode in modes() {
        let (_session, handle, m) = demarrer(mode);
        let id = ajouter(&handle, &m, true);
        handle
            .execute(Command::DeleteColumn { column: m.x })
            .unwrap();
        let perdu = handle.snapshot().unwrap();
        assert_eq!(
            perdu.tree.calculation(id).unwrap().stale,
            Some(Staleness::InputMissing),
            "{mode:?}"
        );
        assert!((pente(&perdu, id) - 10.0).abs() < 1e-9, "valeurs gardées");

        handle.undo().unwrap();
        let retrouve = handle.snapshot().unwrap();
        assert_eq!(
            retrouve.tree.calculation(id).unwrap().stale,
            None,
            "{mode:?}"
        );
    }
}

#[test]
fn une_formule_nourrit_un_calcul_et_un_calcul_nourrit_une_formule() {
    for mode in modes() {
        let (_session, handle, m) = demarrer(mode);
        let id = ajouter(&handle, &m, true);
        // Une colonne-formule lit la sortie « fit » du calcul (même feuille).
        let sortie = {
            let snapshot = handle.snapshot().unwrap();
            let calc = snapshot.tree.calculation(id).unwrap();
            calc.outputs
                .iter()
                .find(|o| o.key == "fit")
                .unwrap()
                .column
                .unwrap()
        };
        let snapshot = handle.snapshot().unwrap();
        let colonnes = snapshot.tree.children(m.sheet).unwrap().to_vec();
        let lettre = colonnes.iter().position(|&c| c == sortie).unwrap();
        let nom = |i: usize| ((b'A' + i as u8) as char).to_string();
        handle
            .execute(Command::AddColumn {
                sheet: m.sheet,
                index: None,
                name: Some("Double".into()),
                column: Column::new(stokesforge_core::ColumnType::Numeric),
            })
            .unwrap();
        let double = *handle
            .snapshot()
            .unwrap()
            .tree
            .children(m.sheet)
            .unwrap()
            .last()
            .unwrap();
        handle
            .execute(Command::SetColumnMeta {
                column: double,
                meta: ColumnMeta {
                    formula: Some(format!("{}*2", nom(lettre))),
                    ..Default::default()
                },
            })
            .unwrap();
        let avant = handle.snapshot().unwrap();
        let lu = |s: &Snapshot| {
            s.tree
                .node(double)
                .unwrap()
                .as_column()
                .unwrap()
                .numeric_view()
                .map(|v| v.to_vec())
        };
        assert!((lu(&avant).unwrap()[3] - 80.0).abs() < 1e-9, "{mode:?}");

        // Changer la source du calcul traverse calcul puis formule.
        handle.execute(saisir(m.y, &[2.0, 4.0, 6.0, 8.0])).unwrap();
        let apres = handle.snapshot().unwrap();
        assert!((lu(&apres).unwrap()[3] - 16.0).abs() < 1e-9, "{mode:?}");
    }
}

#[test]
fn au_chargement_l_automatique_est_recalcule_et_le_manuel_est_cru() {
    let (_session, handle, m) = demarrer(FormulaRecalc::Targeted);
    let auto = ajouter(&handle, &m, true);
    // Un second calcul, manuel, sur les mêmes colonnes.
    let _ = auto;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("p.stf");
    handle.save(&path).unwrap();
    handle.load(&path).unwrap();
    let snapshot = handle.snapshot().unwrap();
    let id = *snapshot.tree.calculations().keys().next().unwrap();
    assert_eq!(snapshot.tree.calculation(id).unwrap().stale, None);
    assert!((pente(&snapshot, id) - 10.0).abs() < 1e-9);

    // Même chose pour un manuel périmé avant la sauvegarde : le marqueur survit.
    let (_session2, handle2, m2) = demarrer(FormulaRecalc::Targeted);
    let manuel = ajouter(&handle2, &m2, false);
    handle2
        .execute(saisir(m2.y, &[2.0, 4.0, 6.0, 8.0]))
        .unwrap();
    let path2 = dir.path().join("q.stf");
    handle2.save(&path2).unwrap();
    handle2.load(&path2).unwrap();
    let recharge = handle2.snapshot().unwrap();
    let calc = recharge.tree.calculation(manuel).unwrap();
    assert_eq!(calc.stale, Some(Staleness::Manual));
    assert!(
        (pente(&recharge, manuel) - 10.0).abs() < 1e-9,
        "valeurs crues"
    );
}

#[test]
fn modifier_un_calcul_manuel_perime_le_met_a_jour_sans_le_reperimer() {
    let (_session, handle, m) = demarrer(FormulaRecalc::Targeted);
    let id = ajouter(&handle, &m, false);
    handle.execute(saisir(m.y, &[2.0, 4.0, 6.0, 8.0])).unwrap();
    assert_eq!(
        handle
            .snapshot()
            .unwrap()
            .tree
            .calculation(id)
            .unwrap()
            .stale,
        Some(Staleness::Manual)
    );
    // « Edit Calculation… » : l'application recalcule les données puis envoie
    // la commande — le calcul est à jour par construction.
    let snapshot = handle.snapshot().unwrap();
    let calc = snapshot.tree.calculation(id).unwrap();
    let computed = compute(&snapshot.tree, &calc.operation, &calc.series, None).unwrap();
    let outputs = computed
        .columns
        .iter()
        .map(|c| OutputDraft {
            series: c.series,
            key: c.key.clone(),
            index: c.index,
            meta: ColumnMeta::default(),
            data: c.data.clone(),
        })
        .collect();
    handle
        .execute(Command::UpdateCalculation {
            id,
            draft: CalculationDraft {
                operation: calc.operation.clone(),
                series: calc.series.clone(),
                aux: None,
                outputs,
                results: computed.results,
                keep_results: true,
                auto_recalc: false,
            },
        })
        .unwrap();
    let apres = handle.snapshot().unwrap();
    assert_eq!(apres.tree.calculation(id).unwrap().stale, None);
    assert!((pente(&apres, id) - 2.0).abs() < 1e-9);
}

/// Une sortie qui n'accepte plus le résultat (un fichier abîmé, un type forcé
/// hors commande) marque le calcul en échec au lieu de rester figée sans signe.
#[test]
fn une_sortie_qui_refuse_le_resultat_marque_le_calcul_en_echec() {
    for mode in modes() {
        let mut project = Project::new("Demo").unwrap();
        let root = project.root();
        project.add_workbook(root, Some("Book1")).unwrap();
        let sheet = project.resolve_path("[Book1]Sheet1").unwrap();
        let x = project
            .add_column(sheet, None, Some("X"), Column::from_f64(&[1.0, 2.0, 3.0]))
            .unwrap();
        let y = project
            .add_column(sheet, None, Some("Y"), Column::from_f64(&[2.0, 4.0, 6.0]))
            .unwrap();
        let series = vec![SeriesRef {
            x: Some(x),
            y,
            sigma: None,
        }];
        let op = ajustement();
        let computed = compute(project.tree(), &op, &series, None).unwrap();
        let outputs = computed
            .columns
            .iter()
            .map(|c| OutputDraft {
                series: c.series,
                key: c.key.clone(),
                index: c.index,
                meta: ColumnMeta::default(),
                data: c.data.clone(),
            })
            .collect();
        let id = project
            .add_calculation(CalculationDraft {
                operation: op,
                series,
                aux: None,
                outputs,
                results: computed.results,
                keep_results: true,
                auto_recalc: true,
            })
            .unwrap();
        let out = project.tree().calculation(id).unwrap().outputs[0]
            .column
            .unwrap();
        project
            .set_column_type(out, stokesforge_core::ColumnType::Mixed)
            .unwrap();

        let session = Session::spawn_with_recalc(project, None, mode).unwrap();
        let handle = session.handle();
        handle.execute(saisir(y, &[1.0, 2.0, 3.0])).unwrap();
        let snapshot = handle.snapshot().unwrap();
        assert!(
            matches!(
                snapshot.tree.calculation(id).unwrap().stale,
                Some(Staleness::Failed(
                    stokesforge_analysis::AnalysisError::OutputUnwritable
                ))
            ),
            "{mode:?} : {:?}",
            snapshot.tree.calculation(id).unwrap().stale
        );
    }
}

#[test]
fn remplacer_actualise_les_dependants_et_annuler_retrouve_leurs_valeurs() {
    use stokesforge_analysis::baseline::{BaselineMethod, BaselineOp};
    let (session, handle, m) = demarrer(FormulaRecalc::Targeted);
    let calc = ajouter(&handle, &m, true);
    let before = handle.snapshot().unwrap();
    let initial = colonne_fit(&before, calc);
    let series = vec![SeriesRef {
        x: Some(m.x),
        y: m.y,
        sigma: None,
    }];
    let operation = Operation::Baseline {
        method: BaselineMethod::Line {
            p1: (0., 1.),
            p2: (100., 1.),
        },
        op: BaselineOp::Subtract,
        keep_baseline: false,
    };
    let computed = compute(&before.tree, &operation, &series, None).unwrap();
    let outputs = computed
        .columns
        .into_iter()
        .map(|c| OutputDraft {
            series: c.series,
            key: c.key,
            index: c.index,
            meta: ColumnMeta::default(),
            data: c.data,
        })
        .collect();
    let expected_versions = [m.x, m.y]
        .map(|id| {
            (
                id,
                before.tree.get(id).unwrap().as_column().unwrap().version(),
            )
        })
        .to_vec();
    handle
        .execute(Command::ReplaceAnalysisData {
            draft: CalculationDraft {
                operation,
                series,
                aux: None,
                outputs,
                results: computed.results,
                keep_results: true,
                auto_recalc: false,
            },
            expected_versions,
        })
        .unwrap();
    let after = handle.snapshot().unwrap();
    let corrected = colonne_fit(&after, calc);
    assert_ne!(initial, corrected);
    for (a, b) in initial.iter().zip(corrected) {
        assert!((a - b - 1.).abs() < 1e-8);
    }
    assert_ne!(
        before.tree.get(m.y).unwrap().as_column().unwrap().version(),
        after.tree.get(m.y).unwrap().as_column().unwrap().version()
    );
    handle.undo().unwrap();
    let undone = handle.snapshot().unwrap();
    assert_eq!(undone.tree.correction_traces().len(), 0);
    for (a, b) in initial.iter().zip(colonne_fit(&undone, calc)) {
        assert!((a - b).abs() < 1e-8);
    }
    session.shutdown().unwrap();
}

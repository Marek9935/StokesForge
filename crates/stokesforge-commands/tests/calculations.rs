//! Les commandes de calcul d'analyse (#181) : une entrée d'historique par
//! geste, annulable et rétablissable, refus d'écrire dans une colonne produite.

use stokesforge_analysis::correct::CorrectOp;
use stokesforge_analysis::ops::Operation;
use stokesforge_commands::{Command, History};
use stokesforge_core::{
    AuxRef, CalculationDraft, Column, ColumnMeta, CoreError, NodeId, OutputDraft, Project,
    SeriesRef, SeriesResult, ValueBlock,
};

struct World {
    project: Project,
    history: History,
    sheet: NodeId,
    x: NodeId,
    y: NodeId,
}

fn world() -> World {
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
    World {
        project,
        history: History::new(),
        sheet,
        x,
        y,
    }
}

fn draft(w: &World, data: Vec<f64>) -> CalculationDraft {
    CalculationDraft {
        operation: Operation::Correct {
            op: CorrectOp::Divide,
        },
        series: vec![SeriesRef {
            x: Some(w.x),
            y: w.y,
            sigma: None,
        }],
        aux: None,
        outputs: vec![OutputDraft {
            series: 0,
            key: "corrected".into(),
            index: None,
            meta: ColumnMeta::default(),
            data,
        }],
        results: vec![SeriesResult::default()],
        keep_results: true,
        auto_recalc: true,
    }
}

fn add(w: &mut World, data: Vec<f64>) {
    let d = draft(w, data);
    w.history
        .execute(&mut w.project, Command::AddCalculation { draft: d })
        .unwrap();
}

fn calc_id(w: &World) -> NodeId {
    *w.project.tree().calculations().keys().next().unwrap()
}

#[test]
fn ajout_annulation_et_retablissement_en_une_seule_entree() {
    let mut w = world();
    let before = w.project.tree().clone();
    add(&mut w, vec![1.0, 2.0, 3.0]);
    assert_eq!(w.history.len(), 1, "colonne et enregistrement : une entrée");
    let after = w.project.tree().clone();
    assert_eq!(after.calculations().len(), 1);
    assert_eq!(w.project.tree().children(w.sheet).unwrap().len(), 3);

    w.history.undo(&mut w.project).unwrap();
    assert_eq!(w.project.tree(), &before);
    assert!(w.project.tree().calculations().is_empty());

    w.history.redo(&mut w.project).unwrap();
    assert_eq!(w.project.tree(), &after);
}

#[test]
fn la_modification_se_defait_par_un_seul_undo() {
    let mut w = world();
    add(&mut w, vec![1.0, 2.0, 3.0]);
    let id = calc_id(&w);
    let created = w.project.tree().clone();
    let update = draft(&w, vec![7.0, 8.0, 9.0]);
    w.history
        .execute(
            &mut w.project,
            Command::UpdateCalculation { id, draft: update },
        )
        .unwrap();
    let out = w.project.tree().calculation(id).unwrap().outputs[0]
        .column
        .unwrap();
    let values = |w: &World| {
        w.project
            .tree()
            .get(out)
            .unwrap()
            .as_column()
            .unwrap()
            .numeric_view()
            .unwrap()
            .to_vec()
    };
    assert_eq!(values(&w), vec![7.0, 8.0, 9.0]);
    w.history.undo(&mut w.project).unwrap();
    assert_eq!(w.project.tree(), &created);
    assert_eq!(values(&w), vec![1.0, 2.0, 3.0]);
}

#[test]
fn detacher_garde_les_colonnes_et_supprimer_les_emporte() {
    let mut w = world();
    add(&mut w, vec![1.0, 2.0, 3.0]);
    let id = calc_id(&w);
    let out = w.project.tree().calculation(id).unwrap().outputs[0]
        .column
        .unwrap();
    let with_calc = w.project.tree().clone();

    w.history
        .execute(
            &mut w.project,
            Command::RemoveCalculation {
                id,
                with_columns: false,
            },
        )
        .unwrap();
    assert!(w.project.tree().calculations().is_empty());
    assert!(w.project.tree().node(out).is_some());
    w.history.undo(&mut w.project).unwrap();
    assert_eq!(w.project.tree(), &with_calc);

    w.history
        .execute(
            &mut w.project,
            Command::RemoveCalculation {
                id,
                with_columns: true,
            },
        )
        .unwrap();
    assert!(w.project.tree().node(out).is_none());
    w.history.undo(&mut w.project).unwrap();
    assert_eq!(w.project.tree(), &with_calc);
}

#[test]
fn une_colonne_produite_refuse_la_saisie_jusqu_a_etre_detachee() {
    let mut w = world();
    add(&mut w, vec![1.0, 2.0, 3.0]);
    let id = calc_id(&w);
    let out = w.project.tree().calculation(id).unwrap().outputs[0]
        .column
        .unwrap();
    let write = |w: &mut World| {
        w.history.execute(
            &mut w.project,
            Command::SetCells {
                column: out,
                start: 0,
                values: ValueBlock::Float(vec![99.0]),
                expected_version: None,
            },
        )
    };
    let err = write(&mut w).unwrap_err();
    assert!(
        matches!(
            err,
            stokesforge_commands::CommandError::Core(CoreError::CalculatedColumnReadOnly { column, calculation })
                if column == out && calculation == id
        ),
        "{err:?}"
    );
    // La colonne source reste éditable (témoin : le refus n'est pas général).
    w.history
        .execute(
            &mut w.project,
            Command::SetCells {
                column: w.y,
                start: 0,
                values: ValueBlock::Float(vec![5.0]),
                expected_version: None,
            },
        )
        .unwrap();
    // Détachée, la colonne est ordinaire.
    w.history
        .execute(
            &mut w.project,
            Command::RemoveCalculation {
                id,
                with_columns: false,
            },
        )
        .unwrap();
    write(&mut w).unwrap();
}

#[test]
fn un_echec_ne_laisse_ni_entree_ni_changement() {
    let mut w = world();
    let before = w.project.tree().clone();
    let mut bad = draft(&w, vec![1.0]);
    bad.series[0].y = NodeId::from_raw(404_040);
    assert!(w
        .history
        .execute(&mut w.project, Command::AddCalculation { draft: bad })
        .is_err());
    assert_eq!(w.history.len(), 0);
    assert_eq!(w.project.tree(), &before);
    assert!(w
        .history
        .execute(
            &mut w.project,
            Command::RemoveCalculation {
                id: NodeId::from_raw(404_040),
                with_columns: true
            }
        )
        .is_err());
    assert_eq!(w.history.len(), 0);
}

#[test]
fn une_formule_ne_se_pose_pas_sur_une_colonne_produite() {
    let mut w = world();
    add(&mut w, vec![1.0, 2.0, 3.0]);
    let id = calc_id(&w);
    let out = w.project.tree().calculation(id).unwrap().outputs[0]
        .column
        .unwrap();
    let before = w.project.tree().clone();
    let err = w
        .history
        .execute(
            &mut w.project,
            Command::SetColumnMeta {
                column: out,
                meta: ColumnMeta {
                    formula: Some("A*2".into()),
                    ..Default::default()
                },
            },
        )
        .unwrap_err();
    assert!(matches!(
        err,
        stokesforge_commands::CommandError::Core(CoreError::CalculatedColumnReadOnly { .. })
    ));
    assert_eq!(w.project.tree(), &before);
    // Un autre réglage de métadonnées (nom long, unité) reste permis.
    w.history
        .execute(
            &mut w.project,
            Command::SetColumnMeta {
                column: out,
                meta: ColumnMeta {
                    long_name: "Corrigé".into(),
                    ..Default::default()
                },
            },
        )
        .unwrap();
}

/// Le type et la suppression de cellules changent la forme d'une colonne
/// produite : le recalcul les défairait, ou échouerait sans rien dire.
#[test]
fn le_type_et_la_suppression_de_cellules_d_une_sortie_sont_refuses() {
    let mut w = world();
    add(&mut w, vec![1.0, 2.0, 3.0]);
    let id = calc_id(&w);
    let out = w.project.tree().calculation(id).unwrap().outputs[0]
        .column
        .unwrap();
    let before = w.project.tree().clone();
    for command in [
        Command::SetColumnType {
            column: out,
            column_type: stokesforge_core::ColumnType::Mixed,
        },
        Command::DeleteCells {
            columns: vec![w.x, out],
            at: 0,
            count: 1,
        },
    ] {
        let err = w.history.execute(&mut w.project, command).unwrap_err();
        assert!(
            matches!(
                err,
                stokesforge_commands::CommandError::Core(
                    CoreError::CalculatedColumnReadOnly { .. }
                )
            ),
            "{err:?}"
        );
        assert_eq!(w.project.tree(), &before, "aucun état partiel");
    }
}

/// #212 : les valeurs changent dans la même colonne, sans calcul vivant.
#[test]
fn remplacer_garde_une_trace_et_se_defait_en_un_seul_pas() {
    let mut w = world();
    let before = w.project.tree().clone();
    let d = draft(&w, vec![1., 2., 3.]);
    let versions = [w.x, w.y]
        .map(|id| {
            (
                id,
                w.project
                    .tree()
                    .get(id)
                    .unwrap()
                    .as_column()
                    .unwrap()
                    .version(),
            )
        })
        .to_vec();
    w.history
        .execute(
            &mut w.project,
            Command::ReplaceAnalysisData {
                draft: d,
                expected_versions: versions,
            },
        )
        .unwrap();
    assert_eq!(w.project.tree().children(w.sheet).unwrap().len(), 2);
    assert!(w.project.tree().calculation_of_column(w.y).is_none());
    assert_eq!(w.project.tree().correction_traces().len(), 1);
    let after = w.project.tree().clone();
    assert_ne!(
        before.get(w.y).unwrap().as_column().unwrap().version(),
        after.get(w.y).unwrap().as_column().unwrap().version()
    );
    w.history.undo(&mut w.project).unwrap();
    assert_eq!(w.project.tree(), &before);
    w.history.redo(&mut w.project).unwrap();
    assert_eq!(w.project.tree(), &after);
    w.history
        .execute(
            &mut w.project,
            Command::SetCells {
                column: w.y,
                start: 0,
                values: ValueBlock::Float(vec![99.]),
                expected_version: None,
            },
        )
        .unwrap();
    assert_eq!(
        w.project.tree().correction_traces().len(),
        1,
        "la saisie garde la trace"
    );
}

#[test]
fn remplacement_refuse_longueur_et_versions_sans_ecriture_partielle() {
    let mut w = world();
    let before = w.project.tree().clone();
    let versions = [w.x, w.y]
        .map(|id| {
            (
                id,
                w.project
                    .tree()
                    .get(id)
                    .unwrap()
                    .as_column()
                    .unwrap()
                    .version(),
            )
        })
        .to_vec();
    let d = draft(&w, vec![1.]);
    assert!(w
        .history
        .execute(
            &mut w.project,
            Command::ReplaceAnalysisData {
                draft: d,
                expected_versions: versions.clone()
            }
        )
        .is_err());
    assert_eq!(w.project.tree(), &before);
    assert_eq!(w.history.len(), 0);
    let d = draft(&w, vec![1., 2., 3.]);
    let mut bad = versions;
    bad[0].1 += 1;
    assert!(w
        .history
        .execute(
            &mut w.project,
            Command::ReplaceAnalysisData {
                draft: d,
                expected_versions: bad
            }
        )
        .is_err());
    assert_eq!(w.project.tree(), &before);
    assert_eq!(w.history.len(), 0);
}

#[test]
fn remplacement_preserve_lignes_non_exploitables_et_nan_de_resultat() {
    let mut w = world();
    w.project
        .set_values(w.x, 1, ValueBlock::Float(vec![f64::NAN]))
        .unwrap();
    let d = draft(&w, vec![1., f64::NAN, f64::NAN, 777.]);
    let versions = [w.x, w.y]
        .map(|id| {
            (
                id,
                w.project
                    .tree()
                    .get(id)
                    .unwrap()
                    .as_column()
                    .unwrap()
                    .version(),
            )
        })
        .to_vec();
    w.history
        .execute(
            &mut w.project,
            Command::ReplaceAnalysisData {
                draft: d,
                expected_versions: versions,
            },
        )
        .unwrap();
    let values = w
        .project
        .tree()
        .get(w.y)
        .unwrap()
        .as_column()
        .unwrap()
        .numeric_view()
        .unwrap();
    assert_eq!(values.len(), 3);
    assert_eq!(values[0], 1.);
    assert_eq!(values[1], 20.);
    assert!(values[2].is_nan());
}

#[test]
fn remplacement_dedoublonne_sources_et_refuse_corrections_contradictoires() {
    let mut w = world();
    let before = w.project.tree().clone();
    let mut d = draft(&w, vec![1., 2., 3.]);
    d.series.push(d.series[0].clone());
    d.results.push(SeriesResult::default());
    let mut output = d.outputs[0].clone();
    output.series = 1;
    output.data[0] = 4.;
    d.outputs.push(output);
    let versions = [w.x, w.y]
        .map(|id| {
            (
                id,
                w.project
                    .tree()
                    .get(id)
                    .unwrap()
                    .as_column()
                    .unwrap()
                    .version(),
            )
        })
        .to_vec();
    assert!(w
        .history
        .execute(
            &mut w.project,
            Command::ReplaceAnalysisData {
                draft: d,
                expected_versions: versions
            }
        )
        .is_err());
    assert_eq!(w.project.tree(), &before);
    assert_eq!(w.history.len(), 0);
}

#[test]
fn remplacement_refuse_formule_et_sortie_calculee_puis_supprime_trace_orpheline() {
    let mut w = world();
    let d = draft(&w, vec![1., 2., 3.]);
    let mut versions = [w.x, w.y]
        .map(|id| {
            (
                id,
                w.project
                    .tree()
                    .get(id)
                    .unwrap()
                    .as_column()
                    .unwrap()
                    .version(),
            )
        })
        .to_vec();
    let mut meta = w
        .project
        .tree()
        .get(w.y)
        .unwrap()
        .as_column()
        .unwrap()
        .meta()
        .clone();
    meta.formula = Some("A*2".into());
    w.project.set_column_meta(w.y, meta).unwrap();
    versions.iter_mut().for_each(|(id, version)| {
        *version = w
            .project
            .tree()
            .get(*id)
            .unwrap()
            .as_column()
            .unwrap()
            .version()
    });
    let before = w.project.tree().clone();
    let error = w
        .history
        .execute(
            &mut w.project,
            Command::ReplaceAnalysisData {
                draft: d,
                expected_versions: versions,
            },
        )
        .unwrap_err();
    assert!(matches!(
        error,
        stokesforge_commands::CommandError::Core(CoreError::FormulaColumnReadOnly { .. })
    ));
    assert_eq!(w.project.tree(), &before);
    let mut meta = w
        .project
        .tree()
        .get(w.y)
        .unwrap()
        .as_column()
        .unwrap()
        .meta()
        .clone();
    meta.formula = None;
    w.project.set_column_meta(w.y, meta).unwrap();
    let d = draft(&w, vec![1., 2., 3.]);
    let versions = [w.x, w.y]
        .map(|id| {
            (
                id,
                w.project
                    .tree()
                    .get(id)
                    .unwrap()
                    .as_column()
                    .unwrap()
                    .version(),
            )
        })
        .to_vec();
    w.history
        .execute(
            &mut w.project,
            Command::ReplaceAnalysisData {
                draft: d,
                expected_versions: versions,
            },
        )
        .unwrap();
    w.history
        .execute(&mut w.project, Command::RemoveNode { id: w.y })
        .unwrap();
    assert!(w.project.tree().correction_traces().is_empty());
    w.history.undo(&mut w.project).unwrap();
    assert_eq!(w.project.tree().correction_traces().len(), 1);
    add(&mut w, vec![1., 2., 3.]);
    let out = w.project.tree().calculation(calc_id(&w)).unwrap().outputs[0]
        .column
        .unwrap();
    let mut d = draft(&w, vec![4., 5., 6.]);
    d.series[0].y = out;
    let versions = [w.x, out]
        .map(|id| {
            (
                id,
                w.project
                    .tree()
                    .get(id)
                    .unwrap()
                    .as_column()
                    .unwrap()
                    .version(),
            )
        })
        .to_vec();
    let before = w.project.tree().clone();
    assert!(w
        .history
        .execute(
            &mut w.project,
            Command::ReplaceAnalysisData {
                draft: d,
                expected_versions: versions
            }
        )
        .is_err());
    assert_eq!(w.project.tree(), &before);
}

// ── Référence figée et originaux gardés (#248) ──────────────────────────

/// Un monde avec une colonne de référence (RX, RY) en plus de la série (X, Y).
fn reference_world() -> (World, NodeId, NodeId) {
    let mut w = world();
    let rx = w
        .project
        .add_column(
            w.sheet,
            None,
            Some("RX"),
            Column::from_f64(&[1.0, 2.0, 3.0]),
        )
        .unwrap();
    let ry = w
        .project
        .add_column(
            w.sheet,
            None,
            Some("RY"),
            Column::from_f64(&[2.0, 2.0, 2.0]),
        )
        .unwrap();
    (w, rx, ry)
}

fn corrected_by(w: &World, rx: NodeId, ry: NodeId) -> CalculationDraft {
    CalculationDraft {
        aux: Some(AuxRef { x: Some(rx), y: ry }),
        ..draft(w, vec![5.0, 10.0, 15.0])
    }
}

fn add_corrected(w: &mut World, rx: NodeId, ry: NodeId) {
    let d = corrected_by(w, rx, ry);
    w.history
        .execute(&mut w.project, Command::AddCalculation { draft: d })
        .unwrap();
}

fn values_of(project: &Project, column: NodeId) -> Vec<f64> {
    project
        .tree()
        .get(column)
        .unwrap()
        .as_column()
        .unwrap()
        .numeric_view()
        .unwrap()
        .to_vec()
}

fn store_columns(project: &Project) -> Vec<NodeId> {
    project
        .analysis_store_sheet()
        .map(|sheet| project.tree().children(sheet).unwrap().to_vec())
        .unwrap_or_default()
}

#[test]
fn la_reference_d_une_correction_est_figee_dans_un_classeur_cache() {
    let (mut w, rx, ry) = reference_world();
    add_corrected(&mut w, rx, ry);
    let aux = w
        .project
        .tree()
        .calculation(calc_id(&w))
        .unwrap()
        .aux
        .clone()
        .unwrap();
    assert_ne!(
        aux.y, ry,
        "le calcul lit une copie, pas la colonne d'origine"
    );
    assert!(w.project.is_analysis_copy(aux.y));
    assert!(w.project.is_analysis_copy(aux.x.unwrap()));
    assert_eq!(values_of(&w.project, aux.y), vec![2.0, 2.0, 2.0]);
    assert_eq!(
        store_columns(&w.project).len(),
        2,
        "le Y et son X, rien d'autre"
    );
    // Le classeur est caché et marqué : books() et le Project Manager le taisent.
    let sheet = w.project.analysis_store_sheet().unwrap();
    let book = w.project.tree().parent(sheet).unwrap().unwrap();
    assert!(matches!(
        w.project.tree().get(book).unwrap().kind(),
        stokesforge_core::NodeKind::Workbook { meta, .. } if meta.hidden
    ));
    // La feuille de l'utilisateur n'a gagné que la colonne produite.
    assert_eq!(w.project.tree().children(w.sheet).unwrap().len(), 5);
}

#[test]
fn supprimer_ou_modifier_la_reference_d_origine_ne_casse_rien() {
    let (mut w, rx, ry) = reference_world();
    add_corrected(&mut w, rx, ry);
    let id = calc_id(&w);
    let aux = w
        .project
        .tree()
        .calculation(id)
        .unwrap()
        .aux
        .clone()
        .unwrap();
    w.history
        .execute(
            &mut w.project,
            Command::SetCells {
                column: ry,
                start: 0,
                values: ValueBlock::Float(vec![99.0]),
                expected_version: None,
            },
        )
        .unwrap();
    assert_eq!(
        values_of(&w.project, aux.y),
        vec![2.0, 2.0, 2.0],
        "témoin : copie intacte"
    );
    w.history
        .execute(&mut w.project, Command::RemoveNode { id: ry })
        .unwrap();
    w.history
        .execute(&mut w.project, Command::RemoveNode { id: rx })
        .unwrap();
    let calc = w.project.tree().calculation(id).expect("le calcul reste");
    assert_eq!(calc.aux.as_ref(), Some(&aux));
    assert_eq!(values_of(&w.project, aux.y), vec![2.0, 2.0, 2.0]);
}

#[test]
fn deux_calculs_partagent_la_copie_jusqu_au_dernier() {
    let (mut w, rx, ry) = reference_world();
    add_corrected(&mut w, rx, ry);
    add_corrected(&mut w, rx, ry);
    assert_eq!(
        store_columns(&w.project).len(),
        2,
        "une copie pour les deux"
    );
    let ids: Vec<NodeId> = w.project.tree().calculations().keys().copied().collect();
    let before = w.project.tree().clone();
    w.history
        .execute(
            &mut w.project,
            Command::RemoveCalculation {
                id: ids[0],
                with_columns: false,
            },
        )
        .unwrap();
    assert_eq!(
        store_columns(&w.project).len(),
        2,
        "le second la lit encore"
    );
    w.history
        .execute(
            &mut w.project,
            Command::RemoveCalculation {
                id: ids[1],
                with_columns: true,
            },
        )
        .unwrap();
    assert!(
        w.project.analysis_store_sheet().is_none(),
        "la dernière analyse emporte les copies et leur classeur"
    );
    assert!(w.project.tree().windows().iter().all(|id| {
        !matches!(
            w.project.tree().get(*id).unwrap().kind(),
            stokesforge_core::NodeKind::Workbook { meta, .. } if meta.hidden
        )
    }));
    // Annuler ramène tout, copies comprises.
    w.history.undo(&mut w.project).unwrap();
    w.history.undo(&mut w.project).unwrap();
    assert_eq!(w.project.tree(), &before);
}

#[test]
fn modifier_la_reference_d_un_calcul_fige_la_nouvelle_et_libere_l_ancienne() {
    let (mut w, rx, ry) = reference_world();
    add_corrected(&mut w, rx, ry);
    let id = calc_id(&w);
    let first = w
        .project
        .tree()
        .calculation(id)
        .unwrap()
        .aux
        .clone()
        .unwrap();
    // Un autre état de la référence : une copie neuve.
    w.history
        .execute(
            &mut w.project,
            Command::SetCells {
                column: ry,
                start: 0,
                values: ValueBlock::Float(vec![4.0, 4.0, 4.0]),
                expected_version: None,
            },
        )
        .unwrap();
    let update = corrected_by(&w, rx, ry);
    w.history
        .execute(
            &mut w.project,
            Command::UpdateCalculation { id, draft: update },
        )
        .unwrap();
    let second = w
        .project
        .tree()
        .calculation(id)
        .unwrap()
        .aux
        .clone()
        .unwrap();
    assert_ne!(first.y, second.y);
    assert_eq!(values_of(&w.project, second.y), vec![4.0, 4.0, 4.0]);
    assert!(
        w.project.tree().node(first.y).is_none(),
        "l'ancienne copie ne sert plus"
    );
    assert_eq!(store_columns(&w.project).len(), 2);
    // Rouvrir le calcul sans toucher à la référence garde la copie : le
    // formulaire repasse l'AuxRef du calcul, déjà une copie.
    let reopen = CalculationDraft {
        aux: Some(second.clone()),
        ..draft(&w, vec![5.0, 10.0, 15.0])
    };
    w.history
        .execute(
            &mut w.project,
            Command::UpdateCalculation { id, draft: reopen },
        )
        .unwrap();
    assert_eq!(w.project.tree().calculation(id).unwrap().aux, Some(second));
    assert_eq!(store_columns(&w.project).len(), 2);
}

fn replace_with_reference(w: &mut World, rx: NodeId, ry: NodeId) {
    let d = corrected_by(w, rx, ry);
    let versions = [w.x, w.y, rx, ry]
        .map(|id| {
            let version = w
                .project
                .tree()
                .get(id)
                .unwrap()
                .as_column()
                .unwrap()
                .version();
            (id, version)
        })
        .to_vec();
    w.history
        .execute(
            &mut w.project,
            Command::ReplaceAnalysisData {
                draft: d,
                expected_versions: versions,
            },
        )
        .unwrap();
}

#[test]
fn restaurer_rend_les_originaux_ecrase_les_saisies_et_s_annule() {
    let (mut w, rx, ry) = reference_world();
    replace_with_reference(&mut w, rx, ry);
    assert_eq!(
        values_of(&w.project, w.y),
        vec![5.0, 10.0, 15.0],
        "remplacé"
    );
    let trace = *w.project.tree().correction_traces().keys().next().unwrap();
    // Un original (Y) et la référence figée (Y, X) vivent dans le classeur caché.
    assert_eq!(store_columns(&w.project).len(), 3);
    let record = w.project.tree().correction_traces()[&trace].clone();
    assert_eq!(record.originals.len(), 1);
    assert_eq!(
        values_of(&w.project, record.originals[0].1),
        vec![10.0, 20.0, 30.0]
    );

    // Une saisie faite depuis, et une ligne ajoutée : « il restaure tout ».
    w.history
        .execute(
            &mut w.project,
            Command::SetCells {
                column: w.y,
                start: 0,
                values: ValueBlock::Float(vec![99.0, 98.0, 97.0, 96.0]),
                expected_version: None,
            },
        )
        .unwrap();
    let typed = w.project.tree().clone();
    w.history
        .execute(&mut w.project, Command::RestoreOriginalData { trace })
        .unwrap();
    assert_eq!(values_of(&w.project, w.y), vec![10.0, 20.0, 30.0]);
    assert!(w.project.tree().correction_traces().is_empty());
    assert!(
        w.project.analysis_store_sheet().is_none(),
        "plus de trace : plus de copie"
    );
    let restored = w.project.tree().clone();

    w.history.undo(&mut w.project).unwrap();
    assert_eq!(
        w.project.tree(),
        &typed,
        "Ctrl+Z rend la correction et ses copies"
    );
    assert_eq!(w.project.tree().correction_traces().len(), 1);
    w.history.redo(&mut w.project).unwrap();
    assert_eq!(w.project.tree(), &restored);
}

#[test]
fn supprimer_la_colonne_remplacee_emporte_la_trace_et_les_copies() {
    let (mut w, rx, ry) = reference_world();
    replace_with_reference(&mut w, rx, ry);
    let trace = *w.project.tree().correction_traces().keys().next().unwrap();
    let with_trace = w.project.tree().clone();
    w.history
        .execute(&mut w.project, Command::RemoveNode { id: w.y })
        .unwrap();
    // Plus de ligne de Results, donc plus de bouton : rien à restaurer.
    assert!(w.project.tree().correction_traces().is_empty());
    assert!(w.project.analysis_store_sheet().is_none());
    let error = w
        .history
        .execute(&mut w.project, Command::RestoreOriginalData { trace })
        .unwrap_err();
    assert!(matches!(
        error,
        stokesforge_commands::CommandError::Core(CoreError::UnknownNode(_))
    ));
    // Annuler la suppression ramène la trace, les copies et la restauration.
    w.history.undo(&mut w.project).unwrap();
    assert_eq!(w.project.tree(), &with_trace);
    w.history
        .execute(&mut w.project, Command::RestoreOriginalData { trace })
        .unwrap();
    assert_eq!(values_of(&w.project, w.y), vec![10.0, 20.0, 30.0]);
}

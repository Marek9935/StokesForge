//! Exécution Python sans fenêtre et sauvegarde seulement après succès.
use std::process::Command;

#[cfg(not(feature = "python"))]
#[test]
fn signale_la_feature_absente_sans_interpreteur() {
    let output = Command::new(env!("CARGO_BIN_EXE_stokesforge-cli"))
        .args(["run", "absent.pyst"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8(output.stderr)
        .unwrap()
        .contains("--features python"));
}

#[cfg(feature = "python")]
fn run(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_stokesforge-cli"))
        .args(args)
        .env("PYTHONPATH", "/tmp/stokesforge-piege")
        .env("PYTHONHOME", "/tmp/stokesforge-piege")
        .output()
        .unwrap()
}

#[cfg(feature = "python")]
#[test]
fn calcule_trace_sauve_recharge_et_annule_sans_fenetre() {
    let dir = tempfile::tempdir().unwrap();
    let script = dir.path().join("calcul.pyst");
    let project = dir.path().join("resultat.stf");
    std::fs::write(
        &script,
        r#"
import numpy as np
book = pyst.create_workbook('Résultats')
sheet = pyst.create_worksheet(book, 'Calcul')
x = pyst.create_column(sheet, 'X')
y = pyst.create_column(sheet, 'Y')
pyst.set_values(x, np.arange(3., dtype=np.float64))
pyst.set_values(y, pyst.column(x) ** 2)
pyst.plot(y, x, name='Graph1', title='$x^2$')
print('calcul terminé')
"#,
    )
    .unwrap();
    let out = run(&[
        "run",
        "--out",
        project.to_str().unwrap(),
        script.to_str().unwrap(),
    ]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(String::from_utf8_lossy(&out.stdout).contains("calcul terminé"));
    let loaded = stokesforge_persistence::load(&project).unwrap();
    let y = loaded.resolve_path("[Résultats]Calcul!Y").unwrap();
    assert_eq!(
        loaded
            .tree()
            .node(y)
            .unwrap()
            .as_column()
            .unwrap()
            .float_values()
            .unwrap()
            .as_slice(),
        &[0., 1., 4.]
    );
    assert!(loaded.resolve_path("[Graph1]").is_ok());
    std::fs::write(&script, "y = pyst.resolve('[Résultats]Calcul!Y')\npyst.set_values(y, [42.])\nassert pyst.undo()\nassert pyst.values(y) == (0.,1.,4.)\nprint('annulation vérifiée')").unwrap();
    let out = run(&[
        "run",
        "--project",
        project.to_str().unwrap(),
        script.to_str().unwrap(),
    ]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(String::from_utf8_lossy(&out.stdout).contains("annulation vérifiée"));
}

#[cfg(feature = "python")]
#[test]
fn erreur_conserve_la_sortie_et_necrase_pas_le_fichier() {
    let dir = tempfile::tempdir().unwrap();
    let script = dir.path().join("erreur.pyst");
    let output = dir.path().join("a_preserver.stf");
    std::fs::write(&output, b"sentinelle").unwrap();
    std::fs::write(&script, "import sys\nprint('stdout conservé')\nprint('stderr conservé', file=sys.stderr)\npyst.create_workbook()\nraise RuntimeError('échec volontaire')").unwrap();
    let out = run(&[
        "run",
        "--out",
        output.to_str().unwrap(),
        script.to_str().unwrap(),
    ]);
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stdout).contains("stdout conservé"));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("stderr conservé"));
    assert!(err.contains("Traceback"));
    assert!(err.contains("échec volontaire"));
    assert_eq!(std::fs::read(&output).unwrap(), b"sentinelle");
}

#[cfg(feature = "python")]
#[test]
fn refuse_arguments_et_fichiers_absents_avant_lexecution() {
    for args in [
        vec!["run"],
        vec!["run", "--out"],
        vec!["run", "--inconnu"],
        vec!["run", "a.pyst", "b.pyst"],
        vec!["run", "/n/existe/pas.pyst"],
    ] {
        assert!(!run(&args).status.success(), "{args:?}");
    }
}

/// Un `.stf` dont le seul calcul de correction est rendu illisible dans les
/// octets : la lecture l'écarte.
#[cfg(feature = "python")]
fn write_file_with_a_damaged_calculation(path: &std::path::Path) {
    use stokesforge_analysis::correct::CorrectOp;
    use stokesforge_analysis::ops::Operation;
    use stokesforge_core::{
        CalculationDraft, Column, ColumnMeta, OutputDraft, Project, SeriesRef, SeriesResult,
    };
    let mut project = Project::new("P").unwrap();
    let book = project.add_workbook(project.root(), None).unwrap();
    let sheet = project.tree().children(book).unwrap()[0];
    let y = project
        .add_column(sheet, None, Some("Y"), Column::from_f64(&[1.0, 2.0]))
        .unwrap();
    project
        .add_calculation(CalculationDraft {
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
                data: vec![0.0, 1.0],
            }],
            results: vec![SeriesResult::default()],
            keep_results: true,
            auto_recalc: false,
            low_r2_dialog: false,
        })
        .unwrap();
    stokesforge_persistence::save(&project, path).unwrap();
    let mut bytes = std::fs::read(path).unwrap();
    let at = bytes
        .windows(8)
        .position(|w| w == b"subtract")
        .expect("l'opération est dans le fichier");
    bytes[at..at + 8].copy_from_slice(b"subtraXX");
    std::fs::write(path, bytes).unwrap();
}

/// #309 : `run --project` dit sur stderr ce que la lecture a écarté, puis le
/// script tourne (code de sortie 0) ; un fichier propre ne dit rien.
#[cfg(feature = "python")]
#[test]
fn run_project_avertit_des_ecarts_puis_execute_le_script() {
    let dir = tempfile::tempdir().unwrap();
    let script = dir.path().join("hello.pyst");
    std::fs::write(&script, "print('script exécuté')").unwrap();
    let damaged = dir.path().join("abime.stf");
    write_file_with_a_damaged_calculation(&damaged);
    let out = run(&[
        "run",
        "--project",
        damaged.to_str().unwrap(),
        script.to_str().unwrap(),
    ]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("warning:") && err.contains("discarded"),
        "{err}"
    );
    assert!(String::from_utf8_lossy(&out.stdout).contains("script exécuté"));

    let clean = dir.path().join("propre.stf");
    stokesforge_persistence::save(&stokesforge_core::Project::new("P").unwrap(), &clean).unwrap();
    let out = run(&[
        "run",
        "--project",
        clean.to_str().unwrap(),
        script.to_str().unwrap(),
    ]);
    assert!(out.status.success());
    assert!(!String::from_utf8_lossy(&out.stderr).contains("warning"));
}

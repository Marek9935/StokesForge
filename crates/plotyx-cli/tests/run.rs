//! Exécution Python sans fenêtre et sauvegarde seulement après succès.
use std::process::Command;

#[cfg(not(feature = "python"))]
#[test]
fn signale_la_feature_absente_sans_interpreteur() {
    let output = Command::new(env!("CARGO_BIN_EXE_plotyx-cli"))
        .args(["run", "absent.py"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8(output.stderr)
        .unwrap()
        .contains("--features python"));
}

#[cfg(feature = "python")]
fn run(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_plotyx-cli"))
        .args(args)
        .env("PYTHONPATH", "/tmp/plotyx-piege")
        .env("PYTHONHOME", "/tmp/plotyx-piege")
        .output()
        .unwrap()
}

#[cfg(feature = "python")]
#[test]
fn calcule_trace_sauve_recharge_et_annule_sans_fenetre() {
    let dir = tempfile::tempdir().unwrap();
    let script = dir.path().join("calcul.py");
    let project = dir.path().join("resultat.plx");
    std::fs::write(
        &script,
        r#"
import numpy as np
book = pyx.create_workbook('Résultats')
sheet = pyx.create_worksheet(book, 'Calcul')
x = pyx.create_column(sheet, 'X')
y = pyx.create_column(sheet, 'Y')
pyx.set_values(x, np.arange(3., dtype=np.float64))
pyx.set_values(y, pyx.column(x) ** 2)
pyx.plot(y, x, name='Graph1', title='$x^2$')
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
    let loaded = plotyx_persistence::load(&project).unwrap();
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
    std::fs::write(&script, "y = pyx.resolve('[Résultats]Calcul!Y')\npyx.set_values(y, [42.])\nassert pyx.undo()\nassert pyx.values(y) == (0.,1.,4.)\nprint('annulation vérifiée')").unwrap();
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
    let script = dir.path().join("erreur.py");
    let output = dir.path().join("a_preserver.plx");
    std::fs::write(&output, b"sentinelle").unwrap();
    std::fs::write(&script, "import sys\nprint('stdout conservé')\nprint('stderr conservé', file=sys.stderr)\npyx.create_workbook()\nraise RuntimeError('échec volontaire')").unwrap();
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
        vec!["run", "a.py", "b.py"],
        vec!["run", "/n/existe/pas.py"],
    ] {
        assert!(!run(&args).status.success(), "{args:?}");
    }
}

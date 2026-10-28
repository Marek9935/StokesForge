//! Headless Python execution, saving only after success.
use std::process::Command;

#[cfg(not(feature = "python"))]
#[test]
fn reports_the_missing_feature_without_an_interpreter() {
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
        .env("PYTHONPATH", "/tmp/stokesforge-trap")
        .env("PYTHONHOME", "/tmp/stokesforge-trap")
        .output()
        .unwrap()
}

#[cfg(feature = "python")]
#[test]
fn computes_plots_saves_reloads_and_undoes_without_a_window() {
    let dir = tempfile::tempdir().unwrap();
    let script = dir.path().join("computation.pyst");
    let project = dir.path().join("result.stf");
    std::fs::write(
        &script,
        r#"
import numpy as np
book = pyst.create_workbook('Résultats')
sheet = pyst.create_worksheet(book, 'Calc')
x = pyst.create_column(sheet, 'X')
y = pyst.create_column(sheet, 'Y')
pyst.set_values(x, np.arange(3., dtype=np.float64))
pyst.set_values(y, pyst.column(x) ** 2)
pyst.plot(y, x, name='Graph1', title='$x^2$')
print('computation done')
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
    assert!(String::from_utf8_lossy(&out.stdout).contains("computation done"));
    let loaded = stokesforge_persistence::load(&project).unwrap();
    let y = loaded.resolve_path("[Résultats]Calc!Y").unwrap();
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
    std::fs::write(&script, "y = pyst.resolve('[Résultats]Calc!Y')\npyst.set_values(y, [42.])\nassert pyst.undo()\nassert pyst.values(y) == (0.,1.,4.)\nprint('undo verified')").unwrap();
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
    assert!(String::from_utf8_lossy(&out.stdout).contains("undo verified"));
}

#[cfg(feature = "python")]
#[test]
fn error_keeps_the_output_and_does_not_overwrite_the_file() {
    let dir = tempfile::tempdir().unwrap();
    let script = dir.path().join("error.pyst");
    let output = dir.path().join("to_preserve.stf");
    std::fs::write(&output, b"sentinel").unwrap();
    std::fs::write(&script, "import sys\nprint('stdout kept')\nprint('stderr kept', file=sys.stderr)\npyst.create_workbook()\nraise RuntimeError('deliberate failure')").unwrap();
    let out = run(&[
        "run",
        "--out",
        output.to_str().unwrap(),
        script.to_str().unwrap(),
    ]);
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stdout).contains("stdout kept"));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("stderr kept"));
    assert!(err.contains("Traceback"));
    assert!(err.contains("deliberate failure"));
    assert_eq!(std::fs::read(&output).unwrap(), b"sentinel");
}

#[cfg(feature = "python")]
#[test]
fn refuses_arguments_and_missing_files_before_execution() {
    for args in [
        vec!["run"],
        vec!["run", "--out"],
        vec!["run", "--unknown"],
        vec!["run", "a.pyst", "b.pyst"],
        vec!["run", "/does/not/exist.pyst"],
    ] {
        assert!(!run(&args).status.success(), "{args:?}");
    }
}

/// A `.stf` whose only correction calculation is made unreadable in the
/// bytes: the read discards it.
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
        .expect("the operation is in the file");
    bytes[at..at + 8].copy_from_slice(b"subtraXX");
    std::fs::write(path, bytes).unwrap();
}

/// #309: `run --project` reports on stderr what the read discarded, then the
/// script runs (exit code 0); a clean file reports nothing.
#[cfg(feature = "python")]
#[test]
fn run_project_warns_about_discards_then_runs_the_script() {
    let dir = tempfile::tempdir().unwrap();
    let script = dir.path().join("hello.pyst");
    std::fs::write(&script, "print('script executed')").unwrap();
    let damaged = dir.path().join("damaged.stf");
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
    assert!(String::from_utf8_lossy(&out.stdout).contains("script executed"));

    let clean = dir.path().join("clean.stf");
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

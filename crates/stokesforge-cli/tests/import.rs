//! `stokesforge-cli import` : fichiers d'appareils → projet `.stf`, sans interface.

use std::path::Path;

fn fixture(name: &str) -> String {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../test/import")
        .join(name)
        .display()
        .to_string()
}

fn run(args: &[&str]) -> (Result<(), String>, String) {
    let args: Vec<String> = args.iter().map(|a| a.to_string()).collect();
    let mut out = Vec::new();
    let result = stokesforge_cli::run(&args, &mut out);
    (result, String::from_utf8(out).unwrap())
}

/// Isole le dossier de config : les modèles de la machine n'y sont pas.
fn isolated() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    std::env::set_var("XDG_CONFIG_HOME", dir.path().join("config"));
    dir
}

fn load(path: &Path) -> stokesforge_core::Project {
    stokesforge_persistence::load(path).expect("projet relisible")
}

#[test]
fn imports_a_jasco_file_with_the_model_chosen_from_its_content() {
    let dir = isolated();
    let out_path = dir.path().join("jasco.stf");
    let (result, output) = run(&[
        "import",
        "--out",
        out_path.to_str().unwrap(),
        &fixture("jasco_uvvis_semicolon.csv"),
    ]);
    result.unwrap();
    assert!(output.contains("Jasco (point-virgule)"), "{output}");
    let project = load(&out_path);
    let sheet = project
        .resolve_path("[Import]jasco_uvvis_semicolon")
        .expect("feuille nommée d'après le fichier");
    let columns: Vec<_> = project.tree().node(sheet).unwrap().children().to_vec();
    assert_eq!(columns.len(), 2);
    let first = project
        .tree()
        .node(columns[0])
        .unwrap()
        .as_column()
        .unwrap();
    assert_eq!(first.meta().role, stokesforge_core::ColumnRole::X);
    assert_eq!(first.len(), 6);
    assert!(first.meta().import_source.is_some(), "source mémorisée");
}

#[test]
fn layout_and_named_model_options() {
    let dir = isolated();
    let out_path = dir.path().join("deux.stf");
    let (result, output) = run(&[
        "import",
        "--recipe",
        "RPE (tabulation)",
        "--layout",
        "side-by-side",
        "--out",
        out_path.to_str().unwrap(),
        &fixture("rpe_nohead_tab.asc"),
        &fixture("rpe_nohead_tab.asc"),
    ]);
    result.unwrap();
    assert!(output.contains("1 worksheet, 4 columns"), "{output}");
    assert!(out_path.exists());
}

#[test]
fn errors_are_reported_not_panicked() {
    let _dir = isolated();
    assert!(run(&["import"]).0.unwrap_err().contains("no file"));
    assert!(run(&["import", "--recipe", "Inconnu", "x.csv"])
        .0
        .unwrap_err()
        .contains("Inconnu"));
    assert!(run(&["import", "--layout", "diagonale", "x.csv"])
        .0
        .is_err());
    assert!(run(&["import", "/n/existe/pas.csv"])
        .0
        .unwrap_err()
        .contains("nothing to import"));
    assert!(run(&["frobnicate"])
        .0
        .unwrap_err()
        .contains("unknown command"));
    // Un fichier illisible n'empêche pas les autres.
    let (result, output) = run(&[
        "import",
        "/n/existe/pas.csv",
        &fixture("rpe_nohead_tab.asc"),
    ]);
    result.unwrap();
    assert!(output.contains("skipped"), "{output}");
    let (_, listing) = run(&["recipes"]);
    assert!(listing.contains("Jasco (tabulation)"));
}

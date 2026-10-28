//! No import template is built into the program (#378): with an empty config
//! folder, the CLI knows none, refuses the former built-in names, and still
//! imports a file with the permissive recipe.

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

/// Empty config folder, set once for the whole binary (process state).
fn isolated() -> tempfile::TempDir {
    static CONFIG: std::sync::OnceLock<tempfile::TempDir> = std::sync::OnceLock::new();
    CONFIG.get_or_init(|| {
        let config = tempfile::tempdir().unwrap();
        std::env::set_var(
            stokesforge_import::config_dir::CONFIG_DIR_VARIABLE,
            config.path(),
        );
        std::env::set_var("XDG_CONFIG_HOME", config.path());
        config
    });
    tempfile::tempdir().unwrap()
}

#[test]
fn recipes_lists_nothing_without_a_saved_template() {
    let _dir = isolated();
    let (result, output) = run(&["recipes"]);
    result.unwrap();
    assert!(output.is_empty(), "{output}");
}

#[test]
fn a_former_builtin_name_is_refused() {
    let _dir = isolated();
    let err = run(&[
        "import",
        "--recipe",
        "Jasco (point-virgule)",
        &fixture("jasco_uvvis_semicolon.csv"),
    ])
    .0
    .unwrap_err();
    assert!(err.contains("unknown template"), "{err}");
    assert!(err.contains("no saved template"), "{err}");
}

#[test]
fn a_jasco_file_imports_with_the_permissive_recipe() {
    let dir = isolated();
    let out_path = dir.path().join("jasco.stf");
    let (result, output) = run(&[
        "import",
        "--out",
        out_path.to_str().unwrap(),
        &fixture("jasco_uvvis_semicolon.csv"),
    ]);
    result.unwrap();
    assert!(!output.contains("Jasco"), "{output}");
}

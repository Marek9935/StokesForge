//! `stokesforge-cli import`: instrument files → `.stf` project, without an interface.

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

/// Isolates the config folder: the machine's templates are not in it, the
/// test templates are.
///
/// The ten tests run in parallel in one process: the variable is a
/// *process* state, set **only once**, to a folder kept
/// until the end of the binary (no test relies on the saved
/// templates). `STOKESFORGE_CONFIG_DIR` (#332) is the variable that
/// `directories` does not ignore outside Linux; `XDG_CONFIG_HOME` stays set for
/// the older readers. The returned folder is specific to each test (output).
fn isolated() -> tempfile::TempDir {
    static CONFIG: std::sync::OnceLock<tempfile::TempDir> = std::sync::OnceLock::new();
    CONFIG.get_or_init(|| {
        let config = tempfile::tempdir().unwrap();
        // No template exists in the program (#378): the test templates are
        // saved ones, copied from the fixture.
        std::fs::create_dir_all(config.path().join("config")).unwrap();
        std::fs::copy(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test/import/test_recipes.json"),
            config.path().join("config/import_recipes.json"),
        )
        .unwrap();
        std::env::set_var(
            stokesforge_import::config_dir::CONFIG_DIR_VARIABLE,
            config.path().join("config"),
        );
        std::env::set_var("XDG_CONFIG_HOME", config.path().join("config"));
        config
    });
    tempfile::tempdir().unwrap()
}

fn load(path: &Path) -> stokesforge_core::Project {
    stokesforge_persistence::load(path).expect("readable project")
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
    assert!(output.contains("Jasco (semicolon)"), "{output}");
    let project = load(&out_path);
    let sheet = project
        .resolve_path("[Import]jasco_uvvis_semicolon")
        .expect("worksheet named after the file");
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
    assert!(first.meta().import_source.is_some(), "source remembered");
}

#[test]
fn layout_and_named_model_options() {
    let dir = isolated();
    let out_path = dir.path().join("two.stf");
    let (result, output) = run(&[
        "import",
        "--recipe",
        "EPR (tab)",
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
    assert!(run(&["import", "--recipe", "Unknown", "x.csv"])
        .0
        .unwrap_err()
        .contains("Unknown"));
    assert!(run(&["import", "--layout", "diagonal", "x.csv"]).0.is_err());
    assert!(run(&["import", "/does/not/exist.csv"])
        .0
        .unwrap_err()
        .contains("nothing to import"));
    assert!(run(&["frobnicate"])
        .0
        .unwrap_err()
        .contains("unknown command"));
    // An unreadable file does not stop the others.
    let (result, output) = run(&[
        "import",
        "/does/not/exist.csv",
        &fixture("rpe_nohead_tab.asc"),
    ]);
    result.unwrap();
    assert!(output.contains("skipped"), "{output}");
    let (_, listing) = run(&["recipes"]);
    assert!(listing.contains("Jasco (tab)"));
}

#[test]
fn an_origin_project_keeps_books_names_units_roles_and_horiba_metadata() {
    let dir = isolated();
    let out_path = dir.path().join("origin.stf");
    let (result, output) = run(&[
        "import",
        "--out",
        out_path.to_str().unwrap(),
        // A text file in the same call keeps its own "Import" workbook.
        &fixture("rpe_nohead_tab.asc"),
        &fixture("origin/synthetic.opj"),
    ]);
    result.unwrap();
    assert!(output.contains("Origin book \"Sample book\""), "{output}");
    let project = load(&out_path);
    assert!(project.resolve_path("[Import]rpe_nohead_tab").is_ok());
    // Data sheet only: the "Note" sheet is not imported.
    assert!(project.resolve_path("[Sample book]Note").is_err());
    let sheet = project
        .resolve_path("[Sample book]Data")
        .expect("the Data sheet of the Origin book");
    let columns: Vec<_> = project.tree().node(sheet).unwrap().children().to_vec();
    assert_eq!(columns.len(), 2);
    let meta = |index: usize| {
        project
            .tree()
            .node(columns[index])
            .unwrap()
            .as_column()
            .unwrap()
            .meta()
            .clone()
    };
    let (x, y) = (meta(0), meta(1));
    assert_eq!(x.role, stokesforge_core::ColumnRole::X);
    assert_eq!(y.role, stokesforge_core::ColumnRole::Y);
    assert_eq!(
        (x.long_name.as_str(), x.unit.as_str()),
        ("Wavelength", "nm")
    );
    assert_eq!((y.long_name.as_str(), y.unit.as_str()), ("Signal", "CPS"));
    assert_eq!(y.comment, "first run");
    assert!(y.import_source.is_none());
    assert!(
        y.file_metadata
            .iter()
            .any(|line| line.key == "HJYInfo.ExpSummary.SWVersion" && line.value == "3.0.0.0"),
        "{:?}",
        y.file_metadata
    );
}

#[test]
fn a_truncated_origin_file_names_the_file_and_does_not_panic() {
    let dir = isolated();
    let bytes = std::fs::read(fixture("origin/synthetic.opj")).unwrap();
    let cut = dir.path().join("cut.opj");
    std::fs::write(&cut, &bytes[..bytes.len() / 2]).unwrap();
    let error = run(&["import", cut.to_str().unwrap()]).0.unwrap_err();
    assert!(
        error.contains("cut.opj") && error.contains("truncated"),
        "{error}"
    );
    // Next to a good file, it is only skipped.
    let (result, output) = run(&[
        "import",
        cut.to_str().unwrap(),
        &fixture("origin/synthetic.opj"),
    ]);
    result.unwrap();
    assert!(
        output.contains("skipped: ") && output.contains("cut.opj"),
        "{output}"
    );
}

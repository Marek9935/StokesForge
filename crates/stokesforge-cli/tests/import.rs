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
    // The fixture holds old-format templates: kept, but not picked by
    // themselves (F1 of #488). The detector reads the file.
    assert!(!output.contains("Jasco (semicolon)"), "{output}");
    let project = load(&out_path);
    let sheet = project
        .resolve_path("[Import]Fixture_Jasco_UVVis")
        .expect("worksheet named after the TITLE key");
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

/// A named old template still works.
#[test]
fn a_named_old_template_is_still_used() {
    let dir = isolated();
    let out_path = dir.path().join("named.stf");
    let (result, output) = run(&[
        "import",
        "--recipe",
        "Jasco (semicolon)",
        "--out",
        out_path.to_str().unwrap(),
        &fixture("jasco_uvvis_semicolon.csv"),
    ]);
    result.unwrap();
    assert!(output.contains("Jasco (semicolon)"), "{output}");
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

const IR_JCAMP: &str = "##TITLE=ir\n##JCAMP-DX=4.24\n##DATA TYPE=INFRARED SPECTRUM\n##XUNITS=1/CM\n##YUNITS=ABSORBANCE\n##FIRSTX=400\n##LASTX=403\n##NPOINTS=4\n##XFACTOR=1\n##YFACTOR=1\n##XYDATA=(X++(Y..Y))\n400 1 2 3 4\n##END=\n";

/// #488: without `--recipe` the detector reads the file (the permissive
/// recipe would not decode a JCAMP-DX table).
#[test]
fn a_jcamp_file_without_a_recipe_is_read_by_the_detector() {
    let dir = isolated();
    let input = dir.path().join("ir.jdx");
    std::fs::write(&input, IR_JCAMP).unwrap();
    let out_path = dir.path().join("jcamp.stf");
    let (result, output) = run(&[
        "import",
        "--out",
        out_path.to_str().unwrap(),
        input.to_str().unwrap(),
    ]);
    result.unwrap_or_else(|err| panic!("{err}\n{output}"));
    let project = load(&out_path);
    let sheet = project.resolve_path("[Import]ir").expect("sheet named ir");
    let columns: Vec<_> = project.tree().node(sheet).unwrap().children().to_vec();
    assert_eq!(columns.len(), 2, "{output}");
    let y = project
        .tree()
        .node(columns[1])
        .unwrap()
        .as_column()
        .unwrap();
    assert_eq!(y.len(), 4);
}

/// #488: NMR data is refused with a clear message, naming the file.
#[test]
fn an_nmr_file_fails_with_a_clear_message() {
    let dir = isolated();
    let input = dir.path().join("n.jdx");
    std::fs::write(
        &input,
        "##TITLE=s\n##JCAMP-DX=5\n##DATA TYPE=NMR SPECTRUM\n##FIRSTX=0\n##LASTX=3\n##NPOINTS=4\n##XYDATA=(X++(Y..Y))\n0 1 2 3 4\n##END=\n",
    )
    .unwrap();
    let out_path = dir.path().join("n.stf");
    let error = run(&[
        "import",
        "--out",
        out_path.to_str().unwrap(),
        input.to_str().unwrap(),
    ])
    .0
    .unwrap_err();
    assert!(error.contains("NMR data is not supported"), "{error}");
    assert!(error.contains("n.jdx"), "{error}");
    assert!(!out_path.exists());
}

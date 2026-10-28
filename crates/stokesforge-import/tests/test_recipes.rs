//! The test templates (`test/import/test_recipes.json`, never shipped) read
//! their fixtures, and the automatic choice proposes the right one (or none)
//! for each real file in `test/import/`. No template exists in production
//! code (#378): the fixture is copied into a redirected config folder and
//! loaded through [`RecipeStore::load`].

use std::path::{Path, PathBuf};

use stokesforge_import::{choose_recipe, parse, ImportRecipe, RecipeStore};

fn fixture(name: &str) -> (PathBuf, Vec<u8>) {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../test/import")
        .join(name);
    let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("{path:?}: {e}"));
    (path, bytes)
}

/// Protects the tests in this binary that redirect the config folder
/// (*process* state, shared by the `cargo test` threads).
static CONFIG_ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// The recipes of `test/import/test_recipes.json`, loaded like a user's
/// saved templates.
fn test_recipes() -> Vec<ImportRecipe> {
    let _lock = CONFIG_ENV_LOCK
        .lock()
        .unwrap_or_else(|err| err.into_inner());
    let dir = tempfile::tempdir().unwrap();
    let _restoration = RestoredConfig::redirect(dir.path());
    let (_, bytes) = fixture("test_recipes.json");
    std::fs::write(dir.path().join("import_recipes.json"), bytes).unwrap();
    RecipeStore::load().recipes().to_vec()
}

fn recipe(name: &str) -> ImportRecipe {
    test_recipes()
        .into_iter()
        .find(|r| r.name == name)
        .unwrap_or_else(|| panic!("test template \"{name}\" missing"))
}

fn auto_name(file: &str) -> Option<String> {
    let (path, bytes) = fixture(file);
    let recipes = test_recipes();
    choose_recipe(&recipes, &path, &bytes).map(|r| r.name.clone())
}

#[test]
fn jasco_semicolon_reads_both_decimal_conventions() {
    let jasco = recipe("Jasco (semicolon)");
    let (_, comma) = fixture("jasco_uvvis_semicolon.csv");
    let table = parse(&comma, &jasco).unwrap();
    assert_eq!(table.row_count(), 6);
    assert_eq!(table.columns[0].values[0], 800.0);
    let (_, dot) = fixture("jasco_english_locale_dot.csv");
    let table = parse(&dot, &jasco).unwrap();
    assert_eq!(table.row_count(), 6);
    assert_eq!(table.columns[0].values[0], 600.0);
    assert_eq!(
        stokesforge_import::resolve_series_name(&jasco, &table, "x"),
        "Fixture_Jasco_EnglishLocale"
    );
}

#[test]
fn the_other_test_templates_read_their_fixtures() {
    let (_, fluo) = fixture("jasco_fluo_tab.txt");
    assert_eq!(parse(&fluo, &recipe("Jasco (tab)")).unwrap().row_count(), 6);
    let (_, analytik) = fixture("analytikjena_nohead_semicolon.csv");
    let table = parse(&analytik, &recipe("AnalytikJena (no header)")).unwrap();
    assert_eq!((table.columns.len(), table.row_count()), (2, 20));
    assert_eq!(table.columns[0].values[0], 250.0);
    let (_, rpe) = fixture("rpe_nohead_tab.asc");
    let table = parse(&rpe, &recipe("EPR (tab)")).unwrap();
    assert_eq!((table.columns.len(), table.row_count()), (2, 20));
    let (_, generic) = fixture("generic_named_header.csv");
    let table = parse(&generic, &recipe("CSV with column names")).unwrap();
    assert_eq!(table.columns[0].name.as_deref(), Some("Time"));
}

#[test]
fn automatic_choice_proposes_the_right_model_or_none() {
    assert_eq!(
        auto_name("jasco_uvvis_semicolon.csv").as_deref(),
        Some("Jasco (semicolon)")
    );
    assert_eq!(
        auto_name("jasco_english_locale_dot.csv").as_deref(),
        Some("Jasco (semicolon)")
    );
    assert_eq!(
        auto_name("JascoUV_vis.csv").as_deref(),
        Some("Jasco (semicolon)")
    );
    assert_eq!(
        auto_name("jasco_fluo_tab.txt").as_deref(),
        Some("Jasco (tab)")
    );
    assert_eq!(
        auto_name("rpe_nohead_tab.asc").as_deref(),
        Some("EPR (tab)")
    );
    // Without a signature of its own: never proposed automatically.
    assert_eq!(auto_name("analytikjena_nohead_semicolon.csv"), None);
    assert_eq!(auto_name("generic_named_header.csv"), None);
    assert_eq!(auto_name("proprietary_binary.dat"), None);
}

#[test]
fn the_fixture_loads_exactly_five_named_templates() {
    let names: Vec<String> = test_recipes().into_iter().map(|r| r.name).collect();
    assert_eq!(
        names,
        [
            "Jasco (semicolon)",
            "Jasco (tab)",
            "AnalytikJena (no header)",
            "EPR (tab)",
            "CSV with column names",
        ]
    );
    let semicolon = recipe("Jasco (semicolon)");
    assert_eq!(semicolon.data_end, stokesforge_import::DataEnd::Auto);
    assert_eq!(semicolon.auto_match.content_contains, "ORIGIN;JASCO");
    assert_eq!(
        recipe("Jasco (tab)").auto_match.content_contains,
        "ORIGIN\tJASCO"
    );
    assert_eq!(recipe("EPR (tab)").auto_match.extensions, ["asc"]);
}

/// Sets `STOKESFORGE_CONFIG_DIR` and `XDG_CONFIG_HOME`, restores both.
struct RestoredConfig([(&'static str, Option<std::ffi::OsString>); 2]);

impl RestoredConfig {
    fn redirect(folder: &std::path::Path) -> Self {
        let names = [
            stokesforge_import::config_dir::CONFIG_DIR_VARIABLE,
            "XDG_CONFIG_HOME",
        ];
        let before = names.map(|name| (name, std::env::var_os(name)));
        for name in names {
            std::env::set_var(name, folder);
        }
        Self(before)
    }
}

impl Drop for RestoredConfig {
    fn drop(&mut self) {
        for (name, value) in self.0.iter_mut() {
            match value.take() {
                Some(value) => std::env::set_var(name, value),
                None => std::env::remove_var(name),
            }
        }
    }
}

#[test]
fn a_recipe_survives_export_to_a_file() {
    let original = recipe("Jasco (tab)");
    let text = original.to_json();
    assert_eq!(ImportRecipe::from_json(&text).unwrap(), original);
    assert!(ImportRecipe::from_json("not a recipe").is_err());
}

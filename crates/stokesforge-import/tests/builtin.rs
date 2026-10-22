//! Les modèles fournis d'office lisent leurs fixtures, et le choix automatique
//! propose le bon (ou aucun) pour chaque fichier réel de `test/import/`.

use std::path::{Path, PathBuf};

use stokesforge_import::{builtin_recipes, choose_recipe, parse, ImportRecipe, RecipeStore};

fn fixture(name: &str) -> (PathBuf, Vec<u8>) {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../test/import")
        .join(name);
    let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("{path:?} : {e}"));
    (path, bytes)
}

fn recipe(name: &str) -> ImportRecipe {
    builtin_recipes()
        .into_iter()
        .find(|r| r.name == name)
        .unwrap_or_else(|| panic!("modèle fourni « {name} » absent"))
}

fn auto_name(file: &str) -> Option<String> {
    let (path, bytes) = fixture(file);
    let recipes = builtin_recipes();
    choose_recipe(&recipes, &path, &bytes).map(|r| r.name.clone())
}

#[test]
fn jasco_semicolon_reads_both_decimal_conventions() {
    let jasco = recipe("Jasco (point-virgule)");
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
fn the_other_builtins_read_their_fixtures() {
    let (_, fluo) = fixture("jasco_fluo_tab.txt");
    assert_eq!(
        parse(&fluo, &recipe("Jasco (tabulation)"))
            .unwrap()
            .row_count(),
        6
    );
    let (_, analytik) = fixture("analytikjena_nohead_semicolon.csv");
    let table = parse(&analytik, &recipe("AnalytikJena (sans en-tête)")).unwrap();
    assert_eq!((table.columns.len(), table.row_count()), (2, 20));
    assert_eq!(table.columns[0].values[0], 250.0);
    let (_, rpe) = fixture("rpe_nohead_tab.asc");
    let table = parse(&rpe, &recipe("RPE (tabulation)")).unwrap();
    assert_eq!((table.columns.len(), table.row_count()), (2, 20));
    let (_, generic) = fixture("generic_named_header.csv");
    let table = parse(&generic, &recipe("CSV avec noms de colonnes")).unwrap();
    assert_eq!(table.columns[0].name.as_deref(), Some("Time"));
}

#[test]
fn automatic_choice_proposes_the_right_model_or_none() {
    assert_eq!(
        auto_name("jasco_uvvis_semicolon.csv").as_deref(),
        Some("Jasco (point-virgule)")
    );
    assert_eq!(
        auto_name("jasco_english_locale_dot.csv").as_deref(),
        Some("Jasco (point-virgule)")
    );
    assert_eq!(
        auto_name("JascoUV_vis.csv").as_deref(),
        Some("Jasco (point-virgule)")
    );
    assert_eq!(
        auto_name("jasco_fluo_tab.txt").as_deref(),
        Some("Jasco (tabulation)")
    );
    assert_eq!(
        auto_name("rpe_nohead_tab.asc").as_deref(),
        Some("RPE (tabulation)")
    );
    // Sans signature propre : jamais proposé d'office.
    assert_eq!(auto_name("analytikjena_nohead_semicolon.csv"), None);
    assert_eq!(auto_name("generic_named_header.csv"), None);
    assert_eq!(auto_name("proprietary_binary.dat"), None);
}

/// Protège les tests de ce binaire qui redirigent le dossier de config
/// (état du *processus*, partagé par les threads de `cargo test`).
static CONFIG_ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[test]
fn builtins_are_listed_but_never_written_to_the_user_file() {
    let _lock = CONFIG_ENV_LOCK
        .lock()
        .unwrap_or_else(|err| err.into_inner());
    let dir = tempfile::tempdir().unwrap();
    // Un dossier neuf, donc aucune recette utilisateur : `config_dir` lit
    // `STOKESFORGE_CONFIG_DIR` (#332), `directories` ignore `XDG_CONFIG_HOME`
    // hors Linux. Les valeurs d'avant sont restaurées, même sur panique.
    let _restauration = ConfigRestauree::redirige(dir.path());
    let store = RecipeStore::load_with_builtins();
    assert!(store.recipes().len() >= builtin_recipes().len());
    assert!(RecipeStore::load().recipes().is_empty());
    store.save().unwrap();
    assert!(RecipeStore::load().recipes().is_empty(), "rien d'écrit");

    let mut mine = store.clone();
    let mut edited = recipe("RPE (tabulation)");
    edited.skip_chars = 3;
    mine.upsert(edited);
    mine.save().unwrap();
    let reloaded = RecipeStore::load_with_builtins();
    let rpe: Vec<_> = reloaded
        .recipes()
        .iter()
        .filter(|r| r.name == "RPE (tabulation)")
        .collect();
    assert_eq!(
        rpe.len(),
        1,
        "la version modifiée remplace celle du programme"
    );
    assert_eq!(rpe[0].skip_chars, 3);
}

/// Pose `STOKESFORGE_CONFIG_DIR` et `XDG_CONFIG_HOME`, restaure les deux.
struct ConfigRestauree([(&'static str, Option<std::ffi::OsString>); 2]);

impl ConfigRestauree {
    fn redirige(dossier: &std::path::Path) -> Self {
        let noms = [
            stokesforge_import::config_dir::CONFIG_DIR_VARIABLE,
            "XDG_CONFIG_HOME",
        ];
        let avant = noms.map(|nom| (nom, std::env::var_os(nom)));
        for nom in noms {
            std::env::set_var(nom, dossier);
        }
        Self(avant)
    }
}

impl Drop for ConfigRestauree {
    fn drop(&mut self) {
        for (nom, valeur) in self.0.iter_mut() {
            match valeur.take() {
                Some(valeur) => std::env::set_var(nom, valeur),
                None => std::env::remove_var(nom),
            }
        }
    }
}

#[test]
fn a_recipe_survives_export_to_a_file() {
    let original = recipe("Jasco (tabulation)");
    let text = original.to_json();
    assert_eq!(ImportRecipe::from_json(&text).unwrap(), original);
    assert!(ImportRecipe::from_json("pas une recette").is_err());
}

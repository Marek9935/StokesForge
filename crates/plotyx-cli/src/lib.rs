//! Le CLI headless : pas d'interface, donc utilisable en lot. Une seule
//! commande utile pour l'instant, `import` — lire des fichiers d'appareils avec
//! un modèle (choisi d'après le fichier, ou nommé) et écrire un projet `.plx`.
//!
//! ```text
//! plotyx-cli import [--recipe NOM] [--layout per-file|side-by-side|shared-x]
//!                   [--out projet.plx] FICHIER...
//! plotyx-cli recipes            # liste les modèles connus
//! ```
//!
//! Le projet est construit directement (`Project` neuf, primitives de
//! `plotyx-core`) : pas de session ni d'historique, c'est la construction d'un
//! projet avant toute édition (voir `docs/invariants.md` §1). La conversion des
//! fichiers en colonnes est celle de l'application, `plotyx-importer`.

use std::io::Write;
use std::path::PathBuf;

use plotyx_core::Project;
use plotyx_import::{choose_recipe, FilesLayout, ImportRecipe, RecipeStore};

const USAGE: &str = "usage :
  plotyx-cli import [--recipe NOM] [--layout per-file|side-by-side|shared-x] [--out projet.plx] FICHIER...
  plotyx-cli recipes";

/// Exécute la ligne de commande `args` (sans le nom du programme), en écrivant
/// les résultats dans `out`. `Err` : un message pour l'erreur standard (code de
/// sortie non nul).
pub fn run(args: &[String], out: &mut dyn Write) -> Result<(), String> {
    match args.first().map(String::as_str) {
        None => writeln!(out, "plotyx-cli : rien à faire sans commande.\n{USAGE}")
            .map_err(|err| err.to_string()),
        Some("recipes") => {
            for recipe in RecipeStore::load_with_builtins().recipes() {
                writeln!(out, "{}", recipe.name).map_err(|err| err.to_string())?;
            }
            Ok(())
        }
        Some("import") => import(&args[1..], out),
        Some(other) => Err(format!("commande inconnue « {other} »\n{USAGE}")),
    }
}

struct ImportArgs {
    recipe: Option<String>,
    layout: Option<FilesLayout>,
    output: Option<PathBuf>,
    files: Vec<PathBuf>,
}

fn parse_import_args(args: &[String]) -> Result<ImportArgs, String> {
    let mut parsed = ImportArgs {
        recipe: None,
        layout: None,
        output: None,
        files: Vec::new(),
    };
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        let mut value = |name: &str| {
            iter.next()
                .cloned()
                .ok_or_else(|| format!("{name} attend une valeur\n{USAGE}"))
        };
        match arg.as_str() {
            "--recipe" => parsed.recipe = Some(value("--recipe")?),
            "--out" => parsed.output = Some(PathBuf::from(value("--out")?)),
            "--layout" => {
                parsed.layout = Some(match value("--layout")?.as_str() {
                    "per-file" => FilesLayout::SheetPerFile,
                    "side-by-side" => FilesLayout::SideBySide,
                    "shared-x" => FilesLayout::SharedX,
                    other => return Err(format!("répartition inconnue « {other} »\n{USAGE}")),
                });
            }
            flag if flag.starts_with("--") => {
                return Err(format!("option inconnue « {flag} »\n{USAGE}"))
            }
            file => parsed.files.push(PathBuf::from(file)),
        }
    }
    if parsed.files.is_empty() {
        return Err(format!("aucun fichier à importer\n{USAGE}"));
    }
    Ok(parsed)
}

fn import(args: &[String], out: &mut dyn Write) -> Result<(), String> {
    let args = parse_import_args(args)?;
    let store = RecipeStore::load_with_builtins();
    let named = match &args.recipe {
        Some(name) => Some(
            store
                .recipes()
                .iter()
                .find(|recipe| &recipe.name == name)
                .cloned()
                .ok_or_else(|| {
                    let known: Vec<&str> =
                        store.recipes().iter().map(|r| r.name.as_str()).collect();
                    format!("modèle « {name} » inconnu (connus : {})", known.join(", "))
                })?,
        ),
        None => None,
    };
    // Un modèle par fichier : celui qu'on a nommé, sinon le choix automatique,
    // sinon la recette permissive.
    let mut items = Vec::new();
    for path in &args.files {
        let mut recipe = match &named {
            Some(recipe) => recipe.clone(),
            None => std::fs::read(path)
                .ok()
                .and_then(|bytes| choose_recipe(store.recipes(), path, &bytes).cloned())
                .unwrap_or_else(|| ImportRecipe::new("Sans titre")),
        };
        if let Some(layout) = args.layout {
            recipe.files_layout = layout;
        }
        items.extend(plotyx_importer::import_items(path, &recipe));
    }
    let failures: Vec<&String> = items
        .iter()
        .filter_map(|item| item.table.as_ref().err())
        .collect();
    let layout = items
        .iter()
        .find(|item| item.table.is_ok())
        .map_or(FilesLayout::SheetPerFile, |item| item.recipe.files_layout);
    let plans = plotyx_importer::plan_import(&items, layout);
    if plans.is_empty() {
        let reasons: Vec<&str> = failures.iter().map(|m| m.as_str()).collect();
        return Err(format!("rien à importer — {}", reasons.join(" ; ")));
    }

    let mut project = Project::new("Import").map_err(|err| err.to_string())?;
    let root = project.root();
    let book = project
        .add_empty_workbook(root, Some("Import"))
        .map_err(|err| err.to_string())?;
    let mut column_count = 0usize;
    for plan in &plans {
        let sheet = project
            .add_worksheet(book, Some(&plan.name))
            .or_else(|_| project.add_worksheet(book, None))
            .map_err(|err| err.to_string())?;
        for &planned in &plan.columns {
            if let Some(column) = plotyx_importer::make_import_column(&items, planned, 0) {
                project
                    .add_column(sheet, None, None, column)
                    .map_err(|err| err.to_string())?;
                column_count += 1;
            }
        }
        let recipe = &items[plan.items[0]].recipe.name;
        writeln!(
            out,
            "feuille « {} » : {} colonne(s), modèle « {recipe} »",
            plan.name,
            plan.columns.len()
        )
        .map_err(|err| err.to_string())?;
    }
    for failure in &failures {
        writeln!(out, "ignoré : {failure}").map_err(|err| err.to_string())?;
    }
    writeln!(out, "{} feuille(s), {column_count} colonne(s)", plans.len())
        .map_err(|err| err.to_string())?;
    if let Some(path) = &args.output {
        plotyx_persistence::save(&project, path).map_err(|err| err.to_string())?;
        writeln!(out, "projet écrit : {}", path.display()).map_err(|err| err.to_string())?;
    }
    Ok(())
}

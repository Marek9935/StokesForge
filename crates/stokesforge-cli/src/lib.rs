//! The headless CLI: no interface, so usable in batch. The single
//! `import` command reads instrument files with a template (chosen
//! from the file, or named) and writes a `.stf` project. With the `python`
//! feature, `run` executes a script in a session and saves its result.
//!
//! ```text
//! stokesforge-cli import [--recipe NAME] [--layout per-file|side-by-side|shared-x]
//!                   [--out project.stf] FILE...
//! stokesforge-cli recipes            # lists the known templates
//! stokesforge-cli run [--project input.stf] [--out output.stf] script.pyst
//! ```
//!
//! For `import`, the project is built directly (fresh `Project`, `stokesforge-core`
//! primitives): no session and no history, it is the construction of a
//! project before any edit (see `docs/invariants.md` §1). The conversion of
//! files into columns is the application's, `stokesforge-importer`.
//! `run` (python feature) starts a session without a window: pyst keeps
//! the same actor, commands and undo as in the console. The
//! output file is saved only after a successful run.

use std::io::Write;
use std::path::PathBuf;

use stokesforge_core::Project;
use stokesforge_import::{choose_recipe, FilesLayout, ImportRecipe, RecipeStore};

const USAGE: &str = "usage:
  stokesforge-cli import [--recipe NAME] [--layout per-file|side-by-side|shared-x] [--out project.stf] FILE...
  stokesforge-cli recipes
  stokesforge-cli run [--project input.stf] [--out output.stf] SCRIPT.pyst";

/// The warning (English, stderr) about what a read discarded, `None`
/// for a clean file (#309).
#[cfg(feature = "python")]
fn discarded_warning(
    path: &std::path::Path,
    report: &stokesforge_persistence::LoadReport,
) -> Option<String> {
    if report.is_clean() {
        return None;
    }
    let calculations = report.discarded_calculations.len();
    let corrections = report.discarded_corrections.len();
    Some(format!(
        "warning: {}: {calculations} calculation(s) and {corrections} correction record(s) were discarded",
        path.display()
    ))
}

/// Runs the command line `args` (without the program name), writing
/// the results to `out`. `Err`: a message for standard error (non-zero
/// exit code).
pub fn run(args: &[String], out: &mut dyn Write) -> Result<(), String> {
    match args.first().map(String::as_str) {
        None => writeln!(
            out,
            "stokesforge-cli: nothing to do without a command.\n{USAGE}"
        )
        .map_err(|err| err.to_string()),
        Some("recipes") => {
            for recipe in RecipeStore::load_with_builtins().recipes() {
                writeln!(out, "{}", recipe.name).map_err(|err| err.to_string())?;
            }
            Ok(())
        }
        Some("import") => import(&args[1..], out),
        Some("run") => run_script(&args[1..], out),
        Some("--help" | "help") => writeln!(out, "{USAGE}").map_err(|err| err.to_string()),
        Some(other) => Err(format!("unknown command \"{other}\"\n{USAGE}")),
    }
}

/// "1 column", "2 columns".
fn counted(count: usize, noun: &str) -> String {
    if count == 1 {
        format!("{count} {noun}")
    } else {
        format!("{count} {noun}s")
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
                .ok_or_else(|| format!("{name} expects a value\n{USAGE}"))
        };
        match arg.as_str() {
            "--recipe" => parsed.recipe = Some(value("--recipe")?),
            "--out" => parsed.output = Some(PathBuf::from(value("--out")?)),
            "--layout" => {
                parsed.layout = Some(match value("--layout")?.as_str() {
                    "per-file" => FilesLayout::SheetPerFile,
                    "side-by-side" => FilesLayout::SideBySide,
                    "shared-x" => FilesLayout::SharedX,
                    other => return Err(format!("unknown layout \"{other}\"\n{USAGE}")),
                });
            }
            flag if flag.starts_with("--") => {
                return Err(format!("unknown option \"{flag}\"\n{USAGE}"))
            }
            file => parsed.files.push(PathBuf::from(file)),
        }
    }
    if parsed.files.is_empty() {
        return Err(format!("no file to import\n{USAGE}"));
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
                    format!("unknown template \"{name}\" (known: {})", known.join(", "))
                })?,
        ),
        None => None,
    };
    // One template per file: the one named, otherwise the automatic choice,
    // otherwise the permissive recipe.
    let mut items = Vec::new();
    for path in &args.files {
        let mut recipe = match &named {
            Some(recipe) => recipe.clone(),
            None => std::fs::read(path)
                .ok()
                .and_then(|bytes| choose_recipe(store.recipes(), path, &bytes).cloned())
                .unwrap_or_else(|| ImportRecipe::new("Untitled")),
        };
        if let Some(layout) = args.layout {
            recipe.files_layout = layout;
        }
        items.extend(stokesforge_importer::import_items(path, &recipe));
    }
    let failures: Vec<String> = items
        .iter()
        .filter_map(|item| item.table.as_ref().err())
        .map(ToString::to_string)
        .collect();
    let layout = items
        .iter()
        .find(|item| item.table.is_ok())
        .map_or(FilesLayout::SheetPerFile, |item| item.recipe.files_layout);
    let plans = stokesforge_importer::plan_import(&items, layout);
    if plans.is_empty() {
        let reasons: Vec<&str> = failures.iter().map(String::as_str).collect();
        return Err(format!("nothing to import: {}", reasons.join(" ; ")));
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
            if let Some(column) = stokesforge_importer::make_import_column(&items, planned, 0) {
                project
                    .add_column(sheet, None, None, column)
                    .map_err(|err| err.to_string())?;
                column_count += 1;
            }
        }
        let recipe = &items[plan.items[0]].recipe.name;
        writeln!(
            out,
            "worksheet \"{}\": {}, template \"{recipe}\"",
            plan.name,
            counted(plan.columns.len(), "column")
        )
        .map_err(|err| err.to_string())?;
    }
    for failure in &failures {
        writeln!(out, "skipped: {failure}").map_err(|err| err.to_string())?;
    }
    writeln!(
        out,
        "{}, {}",
        counted(plans.len(), "worksheet"),
        counted(column_count, "column")
    )
    .map_err(|err| err.to_string())?;
    if let Some(path) = &args.output {
        stokesforge_persistence::save(&project, path).map_err(|err| err.to_string())?;
        writeln!(out, "project written: {}", path.display()).map_err(|err| err.to_string())?;
    }
    Ok(())
}

/// The refusal is detectable without loading Python, and explains how to enable it.
#[cfg(not(feature = "python"))]
fn run_script(_args: &[String], _out: &mut dyn Write) -> Result<(), String> {
    Err("Python scripting is unavailable: build stokesforge-cli with --features python".into())
}

#[cfg(feature = "python")]
fn run_script(args: &[String], out: &mut dyn Write) -> Result<(), String> {
    let mut input = None;
    let mut output = None;
    let mut script = None;
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--project" | "--out" => {
                let value = iter
                    .next()
                    .ok_or_else(|| format!("{arg} expects a value\n{USAGE}"))?;
                if arg == "--project" {
                    input = Some(PathBuf::from(value));
                } else {
                    output = Some(PathBuf::from(value));
                }
            }
            "--" => {
                for arg in iter.by_ref() {
                    if script.replace(PathBuf::from(arg)).is_some() {
                        return Err("run expects exactly one script file".into());
                    }
                }
            }
            option if option.starts_with("--") => {
                return Err(format!("unknown option \"{option}\"\n{USAGE}"))
            }
            path => {
                if script.replace(PathBuf::from(path)).is_some() {
                    return Err("run expects exactly one script file".into());
                }
            }
        }
    }
    let path = script.ok_or_else(|| format!("run expects a script file\n{USAGE}"))?;
    let source = std::fs::read_to_string(&path)
        .map_err(|err| format!("cannot read script {}: {err}", path.display()))?;
    let project = match input {
        Some(path) => {
            let (project, _, report) =
                stokesforge_persistence::load_with_report(&path).map_err(|err| err.to_string())?;
            // What the read discarded is reported on stderr (#309): the
            // script runs anyway, without error.
            if let Some(warning) = discarded_warning(&path, &report) {
                eprintln!("{warning}");
            }
            project
        }
        None => Project::new("Script").map_err(|err| err.to_string())?,
    };
    let actor =
        stokesforge_session::Session::spawn(project, None).map_err(|err| err.to_string())?;
    let session = actor.handle();
    let engine = stokesforge_script::ScriptEngine::spawn(
        session.clone(),
        stokesforge_python::interpreter::EmbeddedPython::locate().map_err(|err| err.to_string())?,
    )
    .map_err(|err| err.to_string())?;
    let outcome = engine.run(&source).map_err(|err| err.to_string())?;
    engine.shutdown().map_err(|err| err.to_string())?;
    out.write_all(outcome.stdout.as_bytes())
        .map_err(|err| err.to_string())?;
    std::io::stderr()
        .write_all(outcome.stderr.as_bytes())
        .map_err(|err| err.to_string())?;
    if let Some(error) = outcome.error {
        return Err(error);
    }
    if outcome.interrupted {
        return Err("script interrupted".into());
    }
    if let Some(path) = output {
        session.save(&path).map_err(|err| err.to_string())?;
        writeln!(out, "saved {}", path.display()).map_err(|err| err.to_string())?;
    }
    actor.shutdown().map_err(|err| err.to_string())?;
    Ok(())
}

//! Automatic import configuration. The detector
//! (`stokesforge_import::sniff`) reads every file that arrives, **once**; a
//! saved template whose conditions the file meets is applied at once. This
//! module keeps those choices, hands the screen the « auto » pills with their
//! plain « why » sentences, and keeps one edited recipe per **kind** of file
//! so that a correction made on one file applies to every file of its kind
//! and not to a file of another kind.

use std::collections::HashMap;
use std::path::PathBuf;

use stokesforge_import::props::{norm_key, prop_text, split_kv};
use stokesforge_import::sniff::{
    marker_main_lines, sniff, AutoChoice, AutoWhat, Check, EndKind, Family, KindKey, SepKind,
    Sniff, SniffRefusal, Why,
};
use stokesforge_import::{FileFacts, HeaderProp, ImportRecipe, NameSource, PropField, PropTarget};

use std::sync::Arc;

use super::{AppState, ImportReadError};
use crate::tr;

/// The map of the file shown and the parse of each file, both kept while
/// neither the file nor the recipe changes.
#[derive(Default)]
pub struct ImportCache {
    map: Option<(PathBuf, ImportRecipe, SharedMap)>,
    /// Detected encoding per file (it does not depend on the recipe).
    pub(crate) encoding: HashMap<PathBuf, Option<&'static str>>,
    parsed: HashMap<PathBuf, (ImportRecipe, SharedParse)>,
}

type SharedMap = Arc<Result<stokesforge_import::FileMap, ImportReadError>>;
type SharedParse = Arc<Result<stokesforge_import::ParsedTable, ImportReadError>>;

/// What the detector said about a file.
pub type SniffResult = Result<Sniff, SniffRefusal>;

/// The recipe a file starts from, and where it comes from.
#[derive(Debug, Clone)]
pub struct FileChoice {
    /// The template's recipe, or the detector's.
    pub recipe: ImportRecipe,
    /// The template that was applied, if one met its conditions.
    pub template: Option<String>,
    /// Other templates that meet as many conditions (a tie to check).
    pub tied: Vec<String>,
}

/// Detection results and per-kind recipes of the import window.
#[derive(Default)]
pub struct ImportAuto {
    /// Detection per path, made once when the file arrives.
    pub sniffs: HashMap<PathBuf, SniffResult>,
    /// What each file starts from (template or detection).
    pub choices: HashMap<PathBuf, FileChoice>,
    /// The recipe as edited, per kind of file.
    pub kind_recipes: HashMap<KindKey, ImportRecipe>,
    /// File whose recipe is in `ImportState::recipe`.
    pub current: Option<PathBuf>,
    /// Choices the user took back one by one.
    pub dismissed: Vec<AutoWhat>,
    /// « Accept all » or « Undo all » was pressed: no pill.
    pub pills_hidden: bool,
    /// How many times the detector ran in this window (once per file, again
    /// only for « Detect again »).
    pub sniff_runs: usize,
}

/// An automatic choice that still holds, ready to show.
#[derive(Debug, Clone)]
pub struct AutoPill {
    pub what: AutoWhat,
    pub lines: Vec<usize>,
    /// The « why » sentence.
    pub text: String,
}

/// A check made on the file, as a sentence.
#[derive(Debug, Clone)]
pub struct AutoCheck {
    pub ok: bool,
    pub text: String,
}

/// What the banner and the pills need for the file shown.
#[derive(Debug, Clone)]
pub struct AutoInfo {
    pub family: Option<Family>,
    pub family_name: String,
    pub rows: usize,
    pub columns: usize,
    pub signature: String,
    /// Choices that still hold (shown as pills unless `pills_hidden`).
    pub pills: Vec<AutoPill>,
    pub pills_hidden: bool,
    /// The saved template the file shown is read with (« Auto · name »).
    pub template: Option<String>,
    pub checks: Vec<AutoCheck>,
}

impl AutoInfo {
    /// The pill to show for `what`, if any.
    pub fn pill(&self, what: &AutoWhat) -> Option<&AutoPill> {
        if self.pills_hidden {
            return None;
        }
        self.pills.iter().find(|pill| &pill.what == what)
    }

    /// Lines of the file that carry a pill (for the gutter of the preview).
    pub fn pill_on_line(&self, line: usize) -> Option<&AutoPill> {
        if self.pills_hidden {
            return None;
        }
        self.pills.iter().find(|pill| {
            pill.lines.first() == Some(&line)
                && !matches!(
                    pill.what,
                    AutoWhat::Separator | AutoWhat::Decimal | AutoWhat::KeepFooter
                )
        })
    }
}

/// A `key;value` line used as a property (« Lu dans l'en-tête »).
#[derive(Debug, Clone)]
pub struct PropRow {
    pub index: usize,
    pub prop: HeaderProp,
    /// 1-based file line the value is read from.
    pub line: Option<usize>,
    /// `key;value` as written in the file.
    pub source: String,
    /// What goes in the field.
    pub result: String,
    /// The pill sentence while the property is still the automatic one.
    pub pill: Option<String>,
    pub pill_what: AutoWhat,
}

/// A choice of the « Take the value as » menu of a `key;value` line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PropChoice {
    LongNameY,
    UnitsX,
    UnitsY,
    CommentY,
    SheetName,
    Nothing,
}

impl PropChoice {
    pub const ALL: [PropChoice; 6] = [
        PropChoice::LongNameY,
        PropChoice::UnitsX,
        PropChoice::UnitsY,
        PropChoice::CommentY,
        PropChoice::SheetName,
        PropChoice::Nothing,
    ];

    fn spec(self) -> Option<(PropField, PropTarget)> {
        match self {
            PropChoice::LongNameY => Some((PropField::LongName, PropTarget::Y)),
            PropChoice::UnitsX => Some((PropField::Units, PropTarget::X)),
            PropChoice::UnitsY => Some((PropField::Units, PropTarget::Y)),
            PropChoice::CommentY => Some((PropField::Comment, PropTarget::Y)),
            PropChoice::SheetName | PropChoice::Nothing => None,
        }
    }
}

/// The mark shown next to a file when several are chosen.
#[derive(Debug, Clone)]
pub struct FileMark {
    pub path: PathBuf,
    pub refusal: Option<SniffRefusal>,
    /// This is the file shown.
    pub shown: bool,
    /// Same kind as the file shown.
    pub same_kind: bool,
    pub columns: usize,
    /// Columns of the file shown (`None` when it is refused).
    pub shown_columns: Option<usize>,
    /// `(declared, read)` when the header announces another number of points.
    pub points_mismatch: Option<(i64, usize)>,
    /// Kind letter: A is the kind of the file shown.
    pub letter: Option<char>,
    /// One line describing the kind.
    pub tip: String,
}

fn family_name(family: Option<Family>) -> String {
    match family {
        Some(Family::Jasco) => tr!("import-auto-family-jasco"),
        Some(Family::Jcamp) => tr!("import-auto-family-jcamp"),
        Some(Family::PlainTable) => tr!("import-auto-family-table"),
        None => tr!("import-auto-family-text"),
    }
}

fn sep_name(kind: SepKind) -> String {
    match kind {
        SepKind::Semicolon => tr!("import-auto-sep-semicolon"),
        SepKind::Tab => tr!("import-auto-sep-tab"),
        SepKind::Comma => tr!("import-auto-sep-comma"),
        SepKind::Whitespace => tr!("import-auto-sep-space"),
        SepKind::None => tr!("import-auto-sep-none"),
    }
}

fn target_name(target: PropTarget) -> String {
    match target {
        PropTarget::X => "X".to_string(),
        PropTarget::Y => "Y".to_string(),
        PropTarget::All => tr!("import-auto-target-all"),
    }
}

/// The « why » sentence of an automatic choice.
pub fn why_text(choice: &AutoChoice) -> String {
    let first: usize = choice.lines.first().copied().unwrap_or(0);
    match &choice.why {
        Why::Encoding(encoding) => {
            let name: String = format!("{encoding:?}");
            tr!("import-why-encoding", name = name)
        }
        Why::Separator { kind, columns } => {
            let sep: String = sep_name(*kind);
            let columns: usize = *columns;
            tr!(
                "import-why-separator",
                line = first,
                sep = sep,
                columns = columns
            )
        }
        Why::Decimal { comma } => {
            if *comma {
                tr!("import-why-decimal-comma", line = first)
            } else {
                tr!("import-why-decimal-dot", line = first)
            }
        }
        Why::DataStart {
            line,
            rows,
            columns,
        } => {
            let (line, rows, columns): (usize, usize, usize) = (*line, *rows, *columns);
            tr!(
                "import-why-data-start",
                line = line,
                rows = rows,
                columns = columns
            )
        }
        Why::Marker { line, text } => {
            let line: usize = *line;
            let text: String = text.clone();
            tr!("import-why-marker", line = line, text = text)
        }
        Why::XPrime { line, .. } => {
            let line: usize = *line;
            tr!("import-why-xprime", line = line)
        }
        Why::LongNameXPrime { line } => {
            let line: usize = *line;
            tr!("import-why-name-xprime", line = line)
        }
        Why::Names { line } => {
            let line: usize = *line;
            tr!("import-why-names", line = line)
        }
        Why::LongName { line } => {
            let line: usize = *line;
            tr!("import-why-long-name", line = line)
        }
        Why::Units { line } => {
            let line: usize = *line;
            tr!("import-why-units", line = line)
        }
        Why::SameLine { line } => {
            let line: usize = *line;
            tr!("import-why-same-line", line = line)
        }
        Why::DataEnd { kind, line } => {
            let line: usize = *line;
            match kind {
                EndKind::BlankLine => tr!("import-why-end-blank", line = line),
                EndKind::Marker => tr!("import-why-end-section", line = line),
                EndKind::Text => tr!("import-why-end-text", line = line),
            }
        }
        Why::KeepFooter { count } => {
            let count: usize = *count;
            tr!("import-why-footer", count = count)
        }
        Why::NameFromKey { key, line } => {
            let line: usize = *line;
            let key: String = key.clone();
            tr!("import-why-sheet-name", line = line, key = key)
        }
        Why::Prop { line, key, value } => {
            let AutoWhat::Prop(prop) = &choice.what else {
                return String::new();
            };
            let line: usize = *line;
            let result: String = prop_text(prop, value).unwrap_or_default();
            let source: String = format!("{key};{value}");
            let target: String = target_name(prop.target);
            match prop.field {
                PropField::Units => tr!(
                    "import-why-prop-units",
                    line = line,
                    source = source,
                    target = target,
                    result = result
                ),
                PropField::LongName => tr!(
                    "import-why-prop-long-name",
                    line = line,
                    source = source,
                    target = target,
                    result = result
                ),
                PropField::Comment => tr!(
                    "import-why-prop-comment",
                    line = line,
                    source = source,
                    target = target,
                    result = result
                ),
            }
        }
    }
}

fn check_text(check: &Check) -> AutoCheck {
    match check {
        Check::NPoints { declared, read } => AutoCheck {
            ok: check.ok(),
            text: if check.ok() {
                {
                    let count: usize = *read;
                    tr!("import-check-points-ok", count = count)
                }
            } else {
                {
                    let (declared, read): (i64, usize) = (*declared, *read);
                    tr!("import-check-points-bad", declared = declared, read = read)
                }
            },
        },
    }
}

/// Choices of the « Take the value as » menu that are on for `key` in `recipe`.
pub fn prop_choices_on(recipe: &ImportRecipe, key: &str) -> Vec<PropChoice> {
    let wanted = norm_key(key);
    let mut on: Vec<PropChoice> = PropChoice::ALL
        .into_iter()
        .filter(|choice| {
            choice.spec().is_some_and(|(field, target)| {
                recipe
                    .header_props
                    .iter()
                    .any(|p| norm_key(&p.key) == wanted && p.field == field && p.target == target)
            })
        })
        .collect();
    if matches!(&recipe.name_source, NameSource::HeaderKey(k) if norm_key(k) == wanted) {
        on.push(PropChoice::SheetName);
    }
    if on.is_empty() {
        on.push(PropChoice::Nothing);
    }
    on
}

impl AppState {
    /// The file shown in the preview.
    pub fn import_shown_path(&self) -> Option<PathBuf> {
        self.import_preview_path()
    }

    /// The whole file shown, line by line with its zones, cached
    /// while neither the file nor the recipe changes.
    pub fn import_file_map(&self) -> Option<SharedMap> {
        let state = self.import_state.as_ref()?;
        let path = self.import_preview_path()?;
        let recipe = state.preview_recipe();
        let mut cache = state.cache.borrow_mut();
        if let Some((cached_path, cached_recipe, map)) = cache.map.as_ref() {
            if *cached_path == path && cached_recipe == recipe {
                return Some(map.clone());
            }
        }
        let result = self.import_bytes(&path).and_then(|bytes| {
            stokesforge_import::file_map(&bytes, recipe).map_err(ImportReadError::Parse)
        });
        let map = Arc::new(result);
        cache.map = Some((path, recipe.clone(), map.clone()));
        Some(map)
    }

    /// The real parse of `path` with the current recipe, cached.
    pub fn import_parsed(&self, path: &std::path::Path) -> SharedParse {
        let Some(state) = self.import_state.as_ref() else {
            return Arc::new(Err(ImportReadError::NoImport));
        };
        let recipe = state.preview_recipe();
        let mut cache = state.cache.borrow_mut();
        if let Some((cached, parsed)) = cache.parsed.get(path) {
            if cached == recipe {
                return parsed.clone();
            }
        }
        let result = self.import_bytes(path).and_then(|bytes| {
            stokesforge_import::parse(&bytes, recipe).map_err(ImportReadError::Parse)
        });
        let parsed = Arc::new(result);
        cache
            .parsed
            .insert(path.to_path_buf(), (recipe.clone(), parsed.clone()));
        parsed
    }

    /// The parse of the file shown.
    pub fn import_parsed_current(&self) -> Option<SharedParse> {
        let path = self.import_preview_path()?;
        Some(self.import_parsed(&path))
    }

    /// Detects every file that has no detection yet, once, and picks what
    /// each file starts from: the template whose conditions it meets
    /// (`RecipeStore::matching`), else the detector's recipe.
    fn auto_sniff_new(&mut self) {
        let Some(state) = self.import_state.as_ref() else {
            return;
        };
        let missing: Vec<PathBuf> = state
            .paths
            .iter()
            .filter(|path| !state.auto.sniffs.contains_key(*path))
            .cloned()
            .collect();
        let mut found = Vec::new();
        for path in missing {
            let name = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or_default()
                .to_string();
            let bytes = self.import_bytes(&path);
            let (result, choice) = match &bytes {
                Ok(bytes) => {
                    let result = sniff(bytes, &name).map(|mut found| {
                        found.recipe.name = tr!(
                            "import-auto-recipe-name",
                            family = family_name(found.family)
                        );
                        found
                    });
                    let facts = FileFacts::new(&path, bytes, result.as_ref().ok());
                    let nmr = matches!(result, Err(SniffRefusal::Nmr));
                    let asked_auto = state.checks.auto_paths.contains(&path);
                    let matched = (!nmr && !asked_auto)
                        .then(|| state.store.matching(&facts))
                        .flatten()
                        .map(|m| {
                            let mut recipe = m.recipe.clone();
                            recipe.unify_header();
                            (
                                recipe,
                                m.recipe.name.clone(),
                                m.tied.iter().map(|r| r.name.clone()).collect::<Vec<_>>(),
                            )
                        });
                    let choice = match (matched, &result) {
                        (Some((recipe, name, tied)), _) => Some(FileChoice {
                            recipe,
                            template: Some(name),
                            tied,
                        }),
                        (None, Ok(found)) => Some(FileChoice {
                            recipe: found.recipe.clone(),
                            template: None,
                            tied: Vec::new(),
                        }),
                        (None, Err(_)) => None,
                    };
                    (result, choice)
                }
                Err(_) => (Err(SniffRefusal::Empty), None),
            };
            found.push((path, result, choice));
        }
        if let Some(state) = self.import_state.as_mut() {
            for (path, result, choice) in found {
                state.auto.sniff_runs += 1;
                if let Some(choice) = choice {
                    match &choice.template {
                        Some(name) => {
                            state
                                .checks
                                .template_of_file
                                .insert(path.clone(), name.clone());
                            if choice.tied.is_empty() {
                                state.checks.tied_of_file.remove(&path);
                            } else {
                                state
                                    .checks
                                    .tied_of_file
                                    .insert(path.clone(), choice.tied.clone());
                            }
                        }
                        None => {
                            state.checks.template_of_file.remove(&path);
                            state.checks.tied_of_file.remove(&path);
                        }
                    }
                    state.auto.choices.insert(path.clone(), choice);
                }
                state.auto.sniffs.insert(path, result);
            }
            state.checks.generation += 1;
        }
    }

    /// Puts the recipe of the file shown in `ImportState::recipe`: the one
    /// being edited for its kind, or else the template's or the detection's.
    /// Called each time the files or the file shown change.
    pub(super) fn auto_sync_shown(&mut self) {
        self.auto_sniff_new();
        let Some(path) = self.import_preview_path() else {
            if let Some(state) = self.import_state.as_mut() {
                state.auto.current = None;
            }
            return;
        };
        let recipe = self.recipe_for_path_unedited(&path);
        let Some(state) = self.import_state.as_mut() else {
            return;
        };
        if state.auto.current.as_ref() == Some(&path) {
            return;
        }
        if let Some(previous) = state.auto.current.take() {
            if let Some(Ok(found)) = state.auto.sniffs.get(&previous) {
                state
                    .auto
                    .kind_recipes
                    .insert(found.kind.clone(), state.recipe.clone());
            }
        }
        if let Some(recipe) = recipe {
            state.seen_recipe = recipe.clone();
            state.recipe = recipe;
        }
        state.auto.current = Some(path);
    }

    fn shown_sniff(&self) -> Option<&SniffResult> {
        let state = self.import_state.as_ref()?;
        let path = self.import_preview_path()?;
        state.auto.sniffs.get(&path)
    }

    /// Why the file shown is refused, if it is. A file a saved template reads
    /// is not refused.
    pub fn import_refusal(&self) -> Option<SniffRefusal> {
        let state = self.import_state.as_ref()?;
        let path = self.import_preview_path()?;
        if state.checks.template_of_file.contains_key(&path) {
            return None;
        }
        match self.shown_sniff()? {
            Err(refusal) => Some(*refusal),
            Ok(_) => None,
        }
    }

    /// What the banner and the pills need, for the file shown.
    pub fn import_auto_info(&self) -> Option<AutoInfo> {
        let state = self.import_state.as_ref()?;
        let Ok(found) = self.shown_sniff()? else {
            return None;
        };
        let path = self.import_preview_path()?;
        let template = state.checks.template_of_file.get(&path).cloned();
        let pills: Vec<AutoPill> = if template.is_some() {
            Vec::new()
        } else {
            found
                .choices
                .iter()
                .filter(|choice| {
                    !state.auto.dismissed.contains(&choice.what)
                        && choice.what.is_set(&state.recipe, &found.recipe)
                })
                .map(|choice| AutoPill {
                    what: choice.what.clone(),
                    lines: choice.lines.clone(),
                    text: why_text(choice),
                })
                .collect()
        };
        Some(AutoInfo {
            family: found.family,
            family_name: family_name(found.family),
            rows: found.rows,
            columns: found.columns,
            signature: found.signature.clone(),
            pills,
            pills_hidden: state.auto.pills_hidden,
            template,
            checks: found.checks.iter().map(check_text).collect(),
        })
    }

    /// « Accept all »: the pills go, the settings stay.
    pub fn import_auto_accept_all(&mut self) {
        if let Some(state) = self.import_state.as_mut() {
            state.auto.pills_hidden = true;
        }
    }

    /// « Undo all »: the plain recipe again, the files kept.
    pub fn import_auto_cancel_all(&mut self) {
        let path = self.import_preview_path();
        if let Some(state) = self.import_state.as_mut() {
            state.recipe = ImportRecipe::new(tr!("import-template-untitled"));
            state.seen_recipe = state.recipe.clone();
            state.auto.pills_hidden = true;
            if let Some(path) = path {
                state.checks.template_of_file.remove(&path);
                state.checks.tied_of_file.remove(&path);
            }
            state.checks.generation += 1;
        }
    }

    /// « Detect again » on the file shown.
    pub fn import_auto_redetect(&mut self) {
        let Some(path) = self.import_preview_path() else {
            return;
        };
        if let Some(state) = self.import_state.as_mut() {
            if let Some(Ok(found)) = state.auto.sniffs.remove(&path) {
                state.auto.kind_recipes.remove(&found.kind);
            }
            state.auto.sniffs.remove(&path);
            state.auto.choices.remove(&path);
            state.checks.forget_file(&path);
            state.auto.current = None;
            state.auto.dismissed.clear();
            state.auto.pills_hidden = false;
        }
        self.auto_sync_shown();
    }

    /// The × of a pill: takes this choice back.
    pub fn import_auto_undo(&mut self, what: &AutoWhat) {
        if let Some(state) = self.import_state.as_mut() {
            what.undo(&mut state.recipe);
            state.auto.dismissed.push(what.clone());
        }
    }

    /// The properties of the recipe with the line they are read from.
    pub fn import_prop_rows(&self) -> Vec<PropRow> {
        let Some(state) = self.import_state.as_ref() else {
            return Vec::new();
        };
        // Read the lines of the cached map instead of decoding the
        // whole file again at each frame.
        let map = self.import_file_map();
        let lines: Vec<&str> = match map.as_deref() {
            Some(Ok(map)) => (0..map.line_count()).map(|i| map.line(i)).collect(),
            _ => Vec::new(),
        };
        let info = self.import_auto_info();
        state
            .recipe
            .header_props
            .iter()
            .enumerate()
            .map(|(index, prop)| {
                let wanted = norm_key(&prop.key);
                let found = lines.iter().enumerate().find_map(|(i, text)| {
                    let (k, v) = split_kv(text)?;
                    (norm_key(k) == wanted && !v.is_empty())
                        .then(|| (i + 1, format!("{};{}", k, v), v.to_string()))
                });
                let what = AutoWhat::Prop(prop.clone());
                let (line, source, result) = match found {
                    Some((line, source, value)) => (
                        Some(line),
                        source,
                        prop_text(prop, &value).unwrap_or_default(),
                    ),
                    None => (None, prop.key.clone(), String::new()),
                };
                PropRow {
                    index,
                    prop: prop.clone(),
                    line,
                    source,
                    result,
                    pill: info
                        .as_ref()
                        .and_then(|i| i.pill(&what))
                        .map(|p| p.text.clone()),
                    pill_what: what,
                }
            })
            .collect()
    }

    /// Choices of the « Take the value as » menu that are on for `key`.
    pub fn import_prop_choices_on(&self, key: &str) -> Vec<PropChoice> {
        self.import_state
            .as_ref()
            .map(|state| prop_choices_on(&state.recipe, key))
            .unwrap_or_default()
    }

    /// Takes the value of `key` as `choice` (or stops using it).
    pub fn import_set_prop_choice(&mut self, key: &str, choice: PropChoice) {
        let Some(state) = self.import_state.as_mut() else {
            return;
        };
        let wanted = norm_key(key);
        let recipe = &mut state.recipe;
        match choice {
            PropChoice::Nothing => {
                recipe.header_props.retain(|p| norm_key(&p.key) != wanted);
                if matches!(&recipe.name_source, NameSource::HeaderKey(k) if norm_key(k) == wanted)
                {
                    recipe.name_source = NameSource::Filename;
                }
            }
            PropChoice::SheetName => {
                let on = matches!(&recipe.name_source, NameSource::HeaderKey(k) if norm_key(k) == wanted);
                recipe.name_source = if on {
                    NameSource::Filename
                } else {
                    NameSource::HeaderKey(key.trim().to_string())
                };
            }
            other => {
                let Some((field, target)) = other.spec() else {
                    return;
                };
                let had = recipe
                    .header_props
                    .iter()
                    .any(|p| norm_key(&p.key) == wanted && p.field == field && p.target == target);
                recipe
                    .header_props
                    .retain(|p| !(p.field == field && p.target == target));
                if !had {
                    recipe.header_props.push(HeaderProp {
                        key: key.trim().to_string(),
                        field,
                        target,
                        short_unit: field == PropField::Units,
                    });
                }
            }
        }
    }

    /// The small combo of a property row: another field and target, or none.
    pub fn import_prop_set_target(&mut self, index: usize, to: Option<(PropField, PropTarget)>) {
        let Some(state) = self.import_state.as_mut() else {
            return;
        };
        if index >= state.recipe.header_props.len() {
            return;
        }
        match to {
            Some((field, target)) => {
                let prop = &mut state.recipe.header_props[index];
                prop.field = field;
                prop.target = target;
                prop.short_unit = field == PropField::Units;
            }
            None => {
                state.recipe.header_props.remove(index);
            }
        }
    }

    /// The marks of the files, when several are chosen.
    pub fn import_file_marks(&self) -> Vec<FileMark> {
        let Some(state) = self.import_state.as_ref() else {
            return Vec::new();
        };
        let shown_path = self.import_preview_path();
        let shown = shown_path
            .as_ref()
            .and_then(|path| state.auto.sniffs.get(path))
            .and_then(|r| r.as_ref().ok());
        // Letters: the kind of the file shown is A, then the others in file order.
        let mut kinds: Vec<KindKey> = Vec::new();
        if let Some(shown) = shown {
            kinds.push(shown.kind.clone());
        }
        for path in &state.paths {
            if let Some(Ok(found)) = state.auto.sniffs.get(path) {
                if !kinds.contains(&found.kind) {
                    kinds.push(found.kind.clone());
                }
            }
        }
        state
            .paths
            .iter()
            .map(|path| {
                let result = state.auto.sniffs.get(path);
                let (refusal, kind, columns, mismatch, tip) = match result {
                    Some(Ok(found)) => {
                        let sep: String = match found.kind.sep {
                            SepKind::Semicolon => ";".to_string(),
                            SepKind::Tab => "tab".to_string(),
                            SepKind::Comma => ",".to_string(),
                            SepKind::Whitespace => "space".to_string(),
                            SepKind::None => "-".to_string(),
                        };
                        let family: String = family_name(found.family);
                        let decimal: String = if found.kind.comma { "," } else { "." }.to_string();
                        let columns: usize = found.columns;
                        let tip: String = tr!(
                            "import-mark-kind-tip",
                            family = family,
                            sep = sep,
                            decimal = decimal,
                            columns = columns
                        );
                        (
                            None,
                            Some(found.kind.clone()),
                            found.columns,
                            found.checks.iter().find(|c| !c.ok()).map(|c| match c {
                                Check::NPoints { declared, read } => (*declared, *read),
                            }),
                            tip,
                        )
                    }
                    Some(Err(refusal)) => (Some(*refusal), None, 0, None, String::new()),
                    None => (None, None, 0, None, String::new()),
                };
                let letter = kind
                    .as_ref()
                    .and_then(|k| kinds.iter().position(|x| x == k))
                    .and_then(|i| char::from_u32('A' as u32 + i as u32));
                FileMark {
                    path: path.clone(),
                    refusal,
                    shown: shown_path.as_ref() == Some(path),
                    same_kind: kind.is_some() && kind.as_ref() == shown.map(|s| &s.kind),
                    columns,
                    shown_columns: shown.map(|s| s.columns),
                    points_mismatch: mismatch,
                    letter,
                    tip,
                }
            })
            .collect()
    }

    /// The recipe a file starts from, before any edit of the window: the one
    /// edited for its kind, else the template's or the detection's. The marker
    /// puts the end of the header where this file has it.
    fn recipe_for_path_unedited(&self, path: &std::path::Path) -> Option<ImportRecipe> {
        let state = self.import_state.as_ref()?;
        let kind_recipe = match state.auto.sniffs.get(path) {
            Some(Ok(found)) => state.auto.kind_recipes.get(&found.kind),
            _ => None,
        };
        let mut recipe = kind_recipe
            .cloned()
            .or_else(|| state.auto.choices.get(path).map(|c| c.recipe.clone()))?;
        if let Ok(bytes) = self.import_bytes(path) {
            if let Some(main) = marker_main_lines(&bytes, &recipe) {
                recipe.header_layout.main_header_lines = main;
            }
        }
        Some(recipe)
    }

    /// The recipe a file would be read with: the one being edited for the file
    /// shown and for the files of its kind, the one of its kind or its own
    /// template for the others.
    pub(super) fn recipe_for_path(&self, path: &std::path::Path) -> ImportRecipe {
        let Some(state) = self.import_state.as_ref() else {
            return ImportRecipe::new(String::new());
        };
        if state.auto.current.as_deref() == Some(path) {
            return state.recipe.clone();
        }
        let current_kind = state
            .auto
            .current
            .as_ref()
            .and_then(|current| state.auto.sniffs.get(current))
            .and_then(|r| r.as_ref().ok())
            .map(|found| &found.kind);
        let same_kind = matches!(
            state.auto.sniffs.get(path),
            Some(Ok(found)) if Some(&found.kind) == current_kind
        );
        if same_kind {
            let mut recipe = state.recipe.clone();
            if let Ok(bytes) = self.import_bytes(path) {
                if let Some(main) = marker_main_lines(&bytes, &recipe) {
                    recipe.header_layout.main_header_lines = main;
                }
            }
            return recipe;
        }
        self.recipe_for_path_unedited(path)
            .unwrap_or_else(|| state.recipe.clone())
    }

    /// For Finish: each readable file with the recipe of its kind (the
    /// corrections made on one file apply to every file of its kind, not to a
    /// file of another kind), and the number of refused files that are
    /// skipped.
    pub fn import_each_recipe(&mut self) -> (Vec<(PathBuf, ImportRecipe)>, usize) {
        // The recipe being edited is stored for the kind of the file shown.
        if let Some(state) = self.import_state.as_mut() {
            if let Some(current) = state.auto.current.clone() {
                if let Some(Ok(found)) = state.auto.sniffs.get(&current) {
                    state
                        .auto
                        .kind_recipes
                        .insert(found.kind.clone(), state.recipe.clone());
                }
            }
        }
        let Some(state) = self.import_state.as_ref() else {
            return (Vec::new(), 0);
        };
        let mut skipped = 0;
        let mut each = Vec::new();
        for path in &state.paths {
            if !state.auto.choices.contains_key(path) {
                skipped += 1;
                continue;
            }
            let mut recipe = self.recipe_for_path(path);
            recipe.import_mode = state.recipe.import_mode;
            recipe.files_layout = state.recipe.files_layout;
            each.push((path.clone(), recipe));
        }
        (each, skipped)
    }

    /// « Import »: the action that imports every readable file of the window,
    /// each with its own recipe, and closes the window. `None` if the window
    /// is not open.
    pub fn import_finish(&mut self) -> Option<super::Action> {
        let destination = self.import_state.as_ref()?.destination;
        let (each, skipped) = self.import_each_recipe();
        self.cancel_import();
        Some(super::Action::StartImportEach {
            destination,
            each,
            skipped,
        })
    }

    // -- First-use guide -----------------------------------------------------

    /// Is the first-use guide open? It opens with the window, every time until
    /// « Don't show again » is ticked, and « Show Guide » brings it back.
    pub fn import_guide_open(&self) -> bool {
        self.import_state.as_ref().is_some_and(|i| i.guide_open)
    }

    /// « Done »: closes the guide for this window. It opens again with the
    /// next window.
    pub fn import_guide_close(&mut self) {
        if let Some(import) = self.import_state.as_mut() {
            import.guide_open = false;
        }
    }

    /// « Don't show again »: closes the guide and keeps the choice in the
    /// preferences. « Show Guide » still brings it back.
    pub fn import_guide_dont_show_again(&mut self) {
        self.import_guide_close();
        if let Err(error) = self
            .object_manager_preferences
            .save_import_guide_hidden(true)
        {
            self.fail(tr!(
                "error-preferences-not-saved",
                cause = crate::errors::io::error_message(&error)
            ));
        }
    }

    /// « Show Guide »: opens the guide, whatever « Don't show again » says.
    pub fn import_guide_show(&mut self) {
        if let Some(import) = self.import_state.as_mut() {
            import.guide_open = true;
        }
    }

    /// Has « Don't show again » been ticked (a preference)?
    pub fn import_guide_hidden(&self) -> bool {
        self.object_manager_preferences.import_guide_hidden()
    }
}

//! Conditions of the saved templates, and the one entry point that picks the
//! recipe of a file (#488).
//!
//! A saved template is a recipe plus the **conditions** a file must meet to be
//! read with it. Pure: no UI, no sentence. The conditions and the answers the
//! user already gave to the detector's doubts live in the [`RecipeStore`], the
//! one template store (`import_recipes.json`).
//!
//! * [`FileFacts`]: what the conditions look at (name, extension, number of
//!   columns, `KEY;value` lines of the header, start of the content).
//! * [`RecipeStore::matching`]: the template that meets the most conditions;
//!   a tie is reported ([`TemplateMatch::tied`]) so the interface can ask.
//! * [`recipe_for_file`]: named template first, then a matching template, then
//!   the detector. Used by the CLI, `pyst.import_file` and the file drop.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use crate::props::split_kv;
use crate::recipe::{AutoMatch, ImportRecipe};
use crate::sniff::{sniff, Doubt, Sniff, SniffRefusal};
use crate::store::RecipeStore;

/// Bytes from the start of the file the header and the content are read from.
const HEAD_BYTES: usize = 64 * 1024;
/// Header lines looked at for `KEY;value`.
const HEAD_LINES: usize = 2000;

/// One condition a file must meet.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Condition {
    /// The number of cells per data line (`2` for a 2D JASCO spectrum, `241`
    /// for a kinetic).
    Columns { at_least: bool, n: usize },
    /// A `KEY;value` line of the header (key compared ignoring case).
    HeaderKey {
        key: String,
        value: String,
        contains: bool,
    },
    /// The name of the file contains a text (case ignored).
    FileName { text: String },
    /// The extension of the file is one of these (no dot, case ignored).
    Extension { exts: Vec<String> },
    /// The start of the file contains a text (case ignored).
    FileStart { text: String },
    /// The file comes from this instrument or software, as the detector names
    /// it (`JASCO V-770`, `JCAMP-DX`; case ignored).
    Origin { name: String },
    /// The `DATA TYPE` of the header (`ULTRAVIOLET SPECTRUM`; case ignored).
    DataType { value: String },
    /// The line that ends the header (`XYDATA`; case ignored).
    Marker { text: String },
    /// How the numbers are written: `sep` is `;`, `,`, `tab` or `space`.
    Numbers { sep: String, comma: bool },
    /// A kinetic: several curves sharing the x column, with an x' line.
    Kinetic,
}

impl Condition {
    /// A condition with nothing to compare with never holds and does not count
    /// as one when saving.
    pub fn is_empty(&self) -> bool {
        match self {
            Condition::Columns { n, .. } => *n == 0,
            Condition::HeaderKey { key, .. } => key.trim().is_empty(),
            Condition::FileName { text } | Condition::FileStart { text } => text.trim().is_empty(),
            Condition::Extension { exts } => exts.iter().all(|e| e.trim().is_empty()),
            Condition::Origin { name } => name.trim().is_empty(),
            Condition::DataType { value } => value.trim().is_empty(),
            Condition::Marker { text } => text.trim().is_empty(),
            Condition::Numbers { sep, .. } => sep.trim().is_empty(),
            Condition::Kinetic => false,
        }
    }

    /// Does the file meet it?
    pub fn holds(&self, facts: &FileFacts) -> bool {
        if self.is_empty() {
            return false;
        }
        match self {
            Condition::Columns { at_least, n } => {
                facts
                    .n_cols
                    .is_some_and(|c| if *at_least { c >= *n } else { c == *n })
            }
            Condition::HeaderKey {
                key,
                value,
                contains,
            } => facts.keys.iter().any(|(k, v)| {
                k.trim().eq_ignore_ascii_case(key.trim())
                    && if *contains {
                        v.to_lowercase().contains(&value.trim().to_lowercase())
                    } else {
                        v.trim().eq_ignore_ascii_case(value.trim())
                    }
            }),
            Condition::FileName { text } => facts
                .file_name
                .to_lowercase()
                .contains(&text.trim().to_lowercase()),
            Condition::Extension { exts } => exts.iter().any(|e| {
                let e = e.trim().trim_start_matches('.');
                !e.is_empty() && facts.extension.eq_ignore_ascii_case(e)
            }),
            Condition::FileStart { text } => facts.head.contains(&text.trim().to_lowercase()),
            Condition::Origin { name } => facts.origin.eq_ignore_ascii_case(name.trim()),
            Condition::DataType { value } => facts.data_type.eq_ignore_ascii_case(value.trim()),
            Condition::Marker { text } => facts.marker.eq_ignore_ascii_case(text.trim()),
            Condition::Numbers { sep, comma } => {
                facts.sep.eq_ignore_ascii_case(sep.trim()) && facts.comma == *comma
            }
            Condition::Kinetic => facts.kinetic,
        }
    }
}

/// The conditions an old template carried as [`AutoMatch`] criteria.
pub fn conditions_from_auto_match(auto: &AutoMatch) -> Vec<Condition> {
    let mut out = Vec::new();
    if !auto.extensions.is_empty() {
        out.push(Condition::Extension {
            exts: auto.extensions.clone(),
        });
    }
    if !auto.name_contains.trim().is_empty() {
        out.push(Condition::FileName {
            text: auto.name_contains.trim().to_string(),
        });
    }
    if !auto.content_contains.trim().is_empty() {
        out.push(Condition::FileStart {
            text: auto.content_contains.trim().to_string(),
        });
    }
    out
}

/// What the conditions look at in a file.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct FileFacts {
    pub file_name: String,
    /// Without the dot, as written.
    pub extension: String,
    /// Cells per data line, when the detector could read the file.
    pub n_cols: Option<usize>,
    /// The `KEY;value` lines of the header, in file order.
    pub keys: Vec<(String, String)>,
    /// Start of the content, lower case.
    head: String,
    /// The kind of file, for the answers remembered ([`kind_id`]); `None`
    /// when the detector refused the file.
    pub kind: Option<String>,
    /// The instrument or software (`JASCO V-770`, `JCAMP-DX`); empty when the
    /// detector does not recognise one.
    pub origin: String,
    /// Raw `DATA TYPE` of the header, upper case; empty when absent.
    pub data_type: String,
    /// The marker line that ends the header, as the detector found it.
    pub marker: String,
    /// The delimiter: `;`, `,`, `tab` or `space`; empty when not sniffed.
    pub sep: String,
    /// Decimal comma.
    pub comma: bool,
    /// Several curves with an x' line.
    pub kinetic: bool,
}

/// `V-770` in `JASCO Corp., V-770, Rev. 1.00`.
fn model_token(text: &str) -> Option<String> {
    text.split(|c: char| c.is_whitespace() || c == ',')
        .map(|t| t.trim_matches(|c: char| c == ',' || c == '.' || c == ';'))
        .find(|t| {
            t.contains('-')
                && t.chars().any(|c| c.is_ascii_digit())
                && t.chars().any(char::is_alphabetic)
                && t.chars().all(|c| c.is_alphanumeric() || c == '-')
        })
        .map(str::to_string)
}

/// The delimiter as the conditions write it.
fn sep_symbol(kind: crate::sniff::SepKind) -> &'static str {
    use crate::sniff::SepKind;
    match kind {
        SepKind::Semicolon => ";",
        SepKind::Comma => ",",
        SepKind::Tab => "tab",
        SepKind::Whitespace | SepKind::None => "space",
    }
}

/// `JASCO V-770`, `JCAMP-DX` or nothing. The model comes from the header key
/// `SPECTROMETER/DATA SYSTEM`, else from the `MODEL NAME` line of the footer.
fn origin_of(sniffed: &Sniff, keys: &[(String, String)], text: &str) -> String {
    use crate::sniff::Family;
    let vendor = match sniffed.family {
        Some(Family::Jasco) => "JASCO",
        Some(Family::Jcamp) => return "JCAMP-DX".to_string(),
        _ => return String::new(),
    };
    let key = |wanted: &str| {
        keys.iter()
            .find(|(k, _)| k.trim().eq_ignore_ascii_case(wanted))
            .map(|(_, v)| v.as_str())
    };
    let model = key("SPECTROMETER/DATA SYSTEM")
        .and_then(model_token)
        .or_else(|| {
            text.lines().take(HEAD_LINES).find_map(|line| {
                let (k, v) = split_kv(line)?;
                (k.trim().eq_ignore_ascii_case("MODEL NAME") && !v.trim().is_empty())
                    .then(|| v.trim().to_string())
            })
        });
    match model {
        Some(model) => format!("{vendor} {model}"),
        None => vendor.to_string(),
    }
}

impl FileFacts {
    /// Facts of a file; `sniffed` is the detector's reading of the same bytes.
    pub fn new(path: &Path, bytes: &[u8], sniffed: Option<&Sniff>) -> Self {
        let head_bytes = &bytes[..bytes.len().min(HEAD_BYTES)];
        let text = crate::decode::decode_text(head_bytes).unwrap_or_default();
        let mut keys = Vec::new();
        for line in text.lines().take(HEAD_LINES) {
            let Some((key, value)) = split_kv(line) else {
                continue;
            };
            // A "key" that is a number is a data line: the header is over.
            if key.replace(',', ".").parse::<f64>().is_ok() {
                break;
            }
            keys.push((key.to_string(), value.to_string()));
        }
        let (origin, data_type, marker, sep, comma, kinetic) = match sniffed {
            Some(found) => {
                let whole;
                let footer_text = if matches!(found.family, Some(crate::sniff::Family::Jasco)) {
                    whole = crate::decode::decode_text(bytes).unwrap_or_default();
                    &whole[..]
                } else {
                    ""
                };
                (
                    origin_of(found, &keys, footer_text),
                    keys.iter()
                        .find(|(k, _)| k.trim().eq_ignore_ascii_case("DATA TYPE"))
                        .map(|(_, v)| v.trim().to_uppercase())
                        .unwrap_or_default(),
                    found.recipe.main_until_marker.clone().unwrap_or_default(),
                    sep_symbol(found.kind.sep).to_string(),
                    found.kind.comma,
                    found.recipe.subheader_roles.x_prime.is_some() && found.columns > 2,
                )
            }
            None => Default::default(),
        };
        Self {
            origin,
            data_type,
            marker,
            sep,
            comma,
            kinetic,
            file_name: path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or_default()
                .to_string(),
            extension: path
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or_default()
                .to_string(),
            n_cols: sniffed.map(|s| s.columns),
            keys,
            head: text.chars().take(4096).collect::<String>().to_lowercase(),
            kind: sniffed.map(kind_id),
        }
    }
}

/// The kind of file a detector reading stands for: two files of one
/// instrument give the same text even when the number of curves differs. It
/// keys the remembered answers.
pub fn kind_id(sniffed: &Sniff) -> String {
    let k = &sniffed.kind;
    format!(
        "{:?}|{:?}|{:?}|{}|{}|{:x}|{:?}|{:?}|{}",
        k.family,
        sniffed.instrument.as_ref().map(|i| i.instrument),
        k.sep,
        k.comma,
        k.columns.min(3),
        k.roles,
        k.header_len,
        k.end,
        sniffed
            .recipe
            .main_until_marker
            .clone()
            .unwrap_or_default()
            .to_uppercase(),
    )
}

impl Doubt {
    /// Stable name of the doubt, for the answers remembered: the kind of doubt
    /// and what it is about, never a block index or a line number, so that two
    /// files of one kind give the same id.
    pub fn id(&self) -> &'static str {
        use crate::jcamp::JcampIssue;
        match self {
            Doubt::TextRowsInData { .. } => "text-rows-in-data",
            Doubt::SeveralBlocks { split: true, .. } => "several-blocks-split",
            Doubt::SeveralBlocks { split: false, .. } => "several-blocks",
            Doubt::DecimalOrSeparator { .. } => "decimal-or-separator",
            Doubt::Jcamp { issue, .. } => match issue {
                JcampIssue::NPointsMismatch { .. } => "jcamp-npoints",
                JcampIssue::NtuplesNotRead => "jcamp-ntuples-not-read",
                JcampIssue::UnreadableTable => "jcamp-unreadable-table",
                JcampIssue::BadLines { .. } => "jcamp-bad-lines",
                JcampIssue::DupCapped => "jcamp-dup-capped",
                JcampIssue::PointsCapped => "jcamp-points-capped",
                JcampIssue::NmrLeftOut { .. } => "jcamp-nmr-left-out",
            },
        }
    }
}

/// Why a template cannot be saved.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SaveIssue {
    NoName,
    /// No usable condition: the template could never be chosen.
    NoCondition,
}

/// The template picked for a file.
#[derive(Debug, Clone, PartialEq)]
pub struct TemplateMatch<'a> {
    pub recipe: &'a ImportRecipe,
    /// How many conditions it meets (all of its own).
    pub met: usize,
    /// The other templates that meet as many conditions: not empty means a
    /// doubt for the interface.
    pub tied: Vec<&'a ImportRecipe>,
}

/// The store side: conditions and remembered answers, both keyed by name or
/// kind. Additive fields of `import_recipes.json`.
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub(crate) struct TemplateExtras {
    /// Conditions per template name.
    #[serde(default)]
    pub(crate) conditions: BTreeMap<String, Vec<Condition>>,
    /// Doubt ids the user answered, per kind of file.
    #[serde(default)]
    pub(crate) remembered: BTreeMap<String, BTreeSet<String>>,
}

impl RecipeStore {
    /// The conditions of a template (empty: never chosen automatically).
    pub fn conditions(&self, name: &str) -> &[Condition] {
        self.extras
            .conditions
            .get(name)
            .map_or(&[][..], Vec::as_slice)
    }

    /// Saves the template with its conditions (same name: replaced). Refused
    /// without a name or without a usable condition.
    pub fn upsert_with_conditions(
        &mut self,
        recipe: ImportRecipe,
        conditions: Vec<Condition>,
    ) -> Result<(), SaveIssue> {
        if recipe.name.trim().is_empty() {
            return Err(SaveIssue::NoName);
        }
        let conditions: Vec<Condition> = conditions.into_iter().filter(|c| !c.is_empty()).collect();
        if conditions.is_empty() {
            return Err(SaveIssue::NoCondition);
        }
        self.extras
            .conditions
            .insert(recipe.name.clone(), conditions);
        self.upsert(recipe);
        Ok(())
    }

    /// Replaces the conditions of an existing template; an empty list makes it
    /// manual-only. `false` if there is no such template.
    pub fn set_conditions(&mut self, name: &str, conditions: Vec<Condition>) -> bool {
        if !self.recipes.iter().any(|r| r.name == name) {
            return false;
        }
        let conditions: Vec<Condition> = conditions.into_iter().filter(|c| !c.is_empty()).collect();
        if conditions.is_empty() {
            self.extras.conditions.remove(name);
        } else {
            self.extras.conditions.insert(name.to_string(), conditions);
        }
        true
    }

    /// Renames a template, its conditions with it. `false` if `from` does not
    /// exist, `to` is blank or already taken.
    pub fn rename(&mut self, from: &str, to: &str) -> bool {
        if to.trim().is_empty() || self.recipes.iter().any(|r| r.name == to) {
            return false;
        }
        let Some(recipe) = self.recipes.iter_mut().find(|r| r.name == from) else {
            return false;
        };
        recipe.name = to.to_string();
        if let Some(conditions) = self.extras.conditions.remove(from) {
            self.extras.conditions.insert(to.to_string(), conditions);
        }
        true
    }

    /// Gives the old `auto_match` criteria of every template without
    /// conditions as conditions (called when the store is read from disk).
    pub(crate) fn adopt_auto_match(&mut self) {
        for recipe in &self.recipes {
            if self.extras.conditions.contains_key(&recipe.name) {
                continue;
            }
            let conditions = conditions_from_auto_match(&recipe.auto_match);
            if !conditions.is_empty() {
                self.extras
                    .conditions
                    .insert(recipe.name.clone(), conditions);
            }
        }
    }

    /// The template the file meets the most conditions of. A template with no
    /// condition is never chosen.
    pub fn matching(&self, facts: &FileFacts) -> Option<TemplateMatch<'_>> {
        let mut best: Vec<(&ImportRecipe, usize)> = Vec::new();
        for recipe in &self.recipes {
            let conditions = self.conditions(&recipe.name);
            if conditions.is_empty() || !conditions.iter().all(|c| c.holds(facts)) {
                continue;
            }
            let met = conditions.len();
            match best.first() {
                Some((_, top)) if met < *top => {}
                Some((_, top)) if met == *top => best.push((recipe, met)),
                _ => best = vec![(recipe, met)],
            }
        }
        let (recipe, met) = *best.first()?;
        Some(TemplateMatch {
            recipe,
            met,
            tied: best.iter().skip(1).map(|(r, _)| *r).collect(),
        })
    }

    /// Remembers that the user answered this doubt for this kind of file.
    pub fn remember_answer(&mut self, kind: &str, doubt: &str) {
        self.extras
            .remembered
            .entry(kind.to_string())
            .or_default()
            .insert(doubt.to_string());
    }

    pub fn is_remembered(&self, kind: &str, doubt: &str) -> bool {
        self.extras
            .remembered
            .get(kind)
            .is_some_and(|set| set.contains(doubt))
    }

    /// Forgets one remembered answer. `false` if there was none.
    pub fn forget_answer(&mut self, kind: &str, doubt: &str) -> bool {
        let Some(set) = self.extras.remembered.get_mut(kind) else {
            return false;
        };
        let removed = set.remove(doubt);
        if set.is_empty() {
            self.extras.remembered.remove(kind);
        }
        removed
    }

    /// Every remembered answer, `(kind of file, doubt id)`, sorted.
    pub fn remembered_answers(&self) -> Vec<(&str, &str)> {
        self.extras
            .remembered
            .iter()
            .flat_map(|(kind, set)| set.iter().map(move |d| (kind.as_str(), d.as_str())))
            .collect()
    }
}

/// Why no recipe could be chosen. `Display` is raw English.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ChooseError {
    #[error("unknown template \"{name}\"")]
    UnknownTemplate { name: String },
    #[error("the file could not be read: {message}")]
    Unreadable { message: String },
    #[error("the file is not readable text (binary file)")]
    Binary,
    #[error("the file is empty")]
    Empty,
    #[error("no line of numbers found in the file")]
    NoNumbers,
    #[error("NMR data is not supported")]
    Nmr,
}

impl From<SniffRefusal> for ChooseError {
    fn from(value: SniffRefusal) -> Self {
        match value {
            SniffRefusal::Binary => ChooseError::Binary,
            SniffRefusal::Empty => ChooseError::Empty,
            SniffRefusal::NoNumbers => ChooseError::NoNumbers,
            SniffRefusal::Nmr => ChooseError::Nmr,
        }
    }
}

/// Where the recipe of a file comes from.
#[derive(Debug, Clone, PartialEq)]
pub enum RecipeOrigin {
    /// The template the caller named.
    Named(String),
    /// A template whose conditions the file meets; `tied` are the others that
    /// meet as many (a doubt to raise).
    Matched { name: String, tied: Vec<String> },
    /// The detector.
    Detected,
}

/// The recipe to read a file with.
#[derive(Debug, Clone, PartialEq)]
pub struct ChosenRecipe {
    pub recipe: ImportRecipe,
    pub origin: RecipeOrigin,
    /// The detector's doubts left after the answers already remembered for
    /// this kind of file; empty when a template decided.
    pub doubts: Vec<Doubt>,
    /// Kind of file, to remember answers under.
    pub kind: Option<String>,
}

/// The one choice of recipe, for the CLI, `pyst.import_file` and the file
/// drop: the template the caller `named`, else the template whose conditions
/// the file meets, else the detector's recipe. NMR data is refused unless a
/// template is named.
pub fn recipe_for_file(
    store: &RecipeStore,
    path: &Path,
    bytes: &[u8],
    named: Option<&str>,
) -> Result<ChosenRecipe, ChooseError> {
    if let Some(name) = named {
        let recipe = store
            .recipes()
            .iter()
            .find(|r| r.name == name)
            .ok_or_else(|| ChooseError::UnknownTemplate { name: name.into() })?;
        return Ok(ChosenRecipe {
            recipe: recipe.clone(),
            origin: RecipeOrigin::Named(name.into()),
            doubts: Vec::new(),
            kind: None,
        });
    }
    let file_name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default();
    let sniffed = sniff(bytes, file_name);
    if matches!(sniffed, Err(SniffRefusal::Nmr)) {
        return Err(ChooseError::Nmr);
    }
    let facts = FileFacts::new(path, bytes, sniffed.as_ref().ok());
    if let Some(found) = store.matching(&facts) {
        return Ok(ChosenRecipe {
            recipe: found.recipe.clone(),
            origin: RecipeOrigin::Matched {
                name: found.recipe.name.clone(),
                tied: found.tied.iter().map(|r| r.name.clone()).collect(),
            },
            doubts: Vec::new(),
            kind: facts.kind,
        });
    }
    let sniffed = sniffed?;
    let kind = facts.kind;
    let doubts = sniffed
        .doubts
        .iter()
        .filter(|d| {
            kind.as_deref()
                .is_none_or(|k| !store.is_remembered(k, d.id()))
        })
        .cloned()
        .collect();
    Ok(ChosenRecipe {
        recipe: sniffed.recipe,
        origin: RecipeOrigin::Detected,
        doubts,
        kind,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn fixture(name: &str) -> (PathBuf, Vec<u8>) {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../test/import")
            .join(name);
        let bytes = std::fs::read(&path).unwrap();
        (path, bytes)
    }

    fn facts(name: &str, cols: Option<usize>) -> FileFacts {
        FileFacts {
            file_name: name.into(),
            extension: name.rsplit_once('.').map_or("", |(_, e)| e).into(),
            n_cols: cols,
            ..FileFacts::default()
        }
    }

    fn cols(n: usize) -> Condition {
        Condition::Columns { at_least: false, n }
    }

    fn store_of(entries: Vec<(&str, Vec<Condition>)>) -> RecipeStore {
        let mut store = RecipeStore::default();
        for (name, conditions) in entries {
            store
                .upsert_with_conditions(ImportRecipe::new(name), conditions)
                .unwrap();
        }
        store
    }

    #[test]
    fn the_number_of_columns_tells_a_2d_spectrum_from_a_kinetic() {
        let store = store_of(vec![
            ("JASCO 2D", vec![cols(2)]),
            (
                "JASCO kinetic",
                vec![Condition::Columns {
                    at_least: true,
                    n: 3,
                }],
            ),
        ]);
        let two = store.matching(&facts("a.csv", Some(2))).unwrap();
        assert_eq!(two.recipe.name, "JASCO 2D");
        let many = store.matching(&facts("a.csv", Some(241))).unwrap();
        assert_eq!(many.recipe.name, "JASCO kinetic");
        // A file the detector could not read has no column count.
        assert!(store.matching(&facts("a.csv", None)).is_none());
    }

    #[test]
    fn each_kind_of_condition_passes_and_fails() {
        let (path, bytes) = fixture("jasco_uvvis_semicolon.csv");
        let sniffed = sniff(&bytes, "jasco_uvvis_semicolon.csv").unwrap();
        let facts = FileFacts::new(&path, &bytes, Some(&sniffed));
        let key = |value: &str, contains| Condition::HeaderKey {
            key: "origin".into(),
            value: value.into(),
            contains,
        };
        assert!(key("JASCO", false).holds(&facts));
        assert!(key("jas", true).holds(&facts));
        assert!(!key("jas", false).holds(&facts));
        assert!(!key("SHIMADZU", false).holds(&facts));
        let name = |text: &str| Condition::FileName { text: text.into() };
        assert!(name("UVVIS").holds(&facts));
        assert!(!name("fluo").holds(&facts));
        let ext = |e: &str| Condition::Extension {
            exts: vec![e.into()],
        };
        assert!(ext(".CSV").holds(&facts));
        assert!(!ext("txt").holds(&facts));
        let start = |t: &str| Condition::FileStart { text: t.into() };
        assert!(start("origin;jasco").holds(&facts));
        assert!(!start("origin;ocean").holds(&facts));
        assert!(cols(facts.n_cols.unwrap()).holds(&facts));
        assert!(!cols(facts.n_cols.unwrap() + 1).holds(&facts));
        // An empty condition never holds.
        assert!(!name("").holds(&facts));
    }

    #[test]
    fn the_automatic_conditions_of_a_detection_pass_and_fail() {
        let (path, bytes) = fixture("jasco_uvvis_semicolon.csv");
        let sniffed = sniff(&bytes, "jasco_uvvis_semicolon.csv").unwrap();
        let facts = FileFacts::new(&path, &bytes, Some(&sniffed));
        assert!(facts.origin.starts_with("JASCO"), "{}", facts.origin);
        let origin = |name: &str| Condition::Origin { name: name.into() };
        assert!(origin(&facts.origin.to_lowercase()).holds(&facts));
        assert!(!origin("JASCO V-999").holds(&facts));
        assert!(!origin("").holds(&facts));
        assert!(!facts.data_type.is_empty());
        let data_type = |value: &str| Condition::DataType {
            value: value.into(),
        };
        assert!(data_type(&facts.data_type.to_lowercase()).holds(&facts));
        assert!(!data_type("FLUORESCENCE SPECTRUM").holds(&facts));
        let numbers = |sep: &str, comma| Condition::Numbers {
            sep: sep.into(),
            comma,
        };
        assert_eq!(facts.sep, ";");
        assert!(numbers(";", facts.comma).holds(&facts));
        assert!(!numbers(";", !facts.comma).holds(&facts));
        assert!(!numbers("tab", facts.comma).holds(&facts));
        assert_eq!(Condition::Kinetic.holds(&facts), facts.kinetic);
        // A file the detector refused meets none of them.
        let blind = FileFacts::new(&path, &bytes, None);
        assert!(!origin(&facts.origin).holds(&blind));
        assert!(!numbers(";", facts.comma).holds(&blind));
        assert!(!Condition::Kinetic.holds(&blind));
        // A marker and a kinetic come from a file that has them.
        let kinetic = FileFacts {
            kinetic: true,
            marker: "XYDATA".into(),
            ..FileFacts::default()
        };
        assert!(Condition::Kinetic.holds(&kinetic));
        assert!(Condition::Marker {
            text: "xydata".into()
        }
        .holds(&kinetic));
        assert!(!Condition::Marker { text: "".into() }.holds(&kinetic));
        assert!(!Condition::Marker {
            text: "other".into()
        }
        .holds(&kinetic));
    }

    #[test]
    fn a_doubt_id_names_the_kind_of_doubt_not_its_block() {
        use crate::jcamp::JcampIssue;
        let jcamp = |block, issue| Doubt::Jcamp { block, issue };
        // Two blocks with the same issue share the id.
        assert_eq!(
            jcamp(0, JcampIssue::NtuplesNotRead).id(),
            jcamp(7, JcampIssue::NtuplesNotRead).id()
        );
        // Different issues, different ids.
        assert_ne!(
            jcamp(0, JcampIssue::NtuplesNotRead).id(),
            jcamp(0, JcampIssue::UnreadableTable).id()
        );
        assert_ne!(
            jcamp(0, JcampIssue::BadLines { count: 1 }).id(),
            jcamp(
                0,
                JcampIssue::NPointsMismatch {
                    declared: 1,
                    decoded: 2
                }
            )
            .id()
        );
        // Line numbers and counts do not enter the id.
        assert_eq!(
            Doubt::DecimalOrSeparator { line: 2 }.id(),
            Doubt::DecimalOrSeparator { line: 90 }.id()
        );
    }

    #[test]
    fn a_template_without_a_usable_condition_is_not_saved() {
        let mut store = RecipeStore::default();
        assert_eq!(
            store.upsert_with_conditions(ImportRecipe::new("X"), vec![]),
            Err(SaveIssue::NoCondition)
        );
        assert_eq!(
            store.upsert_with_conditions(
                ImportRecipe::new("X"),
                vec![Condition::FileName { text: "  ".into() }]
            ),
            Err(SaveIssue::NoCondition)
        );
        assert_eq!(
            store.upsert_with_conditions(ImportRecipe::new(" "), vec![cols(2)]),
            Err(SaveIssue::NoName)
        );
        assert!(store.recipes().is_empty());
    }

    #[test]
    fn an_old_store_turns_its_auto_match_into_conditions() {
        let mut recipe = ImportRecipe::new("Old");
        recipe.auto_match = AutoMatch {
            extensions: vec!["csv".into()],
            name_contains: "uvvis".into(),
            content_contains: "ORIGIN;JASCO".into(),
        };
        // The JSON of a store written before the conditions: only `recipes`.
        let json = serde_json::json!({ "recipes": [recipe] }).to_string();
        let store = RecipeStore::from_json(&json).unwrap();
        assert_eq!(store.conditions("Old").len(), 3);
        let (path, bytes) = fixture("jasco_uvvis_semicolon.csv");
        let facts = FileFacts::new(&path, &bytes, None);
        assert_eq!(store.matching(&facts).unwrap().recipe.name, "Old");
        let (path, bytes) = fixture("jasco_fluo_tab.txt");
        assert!(store
            .matching(&FileFacts::new(&path, &bytes, None))
            .is_none());
    }

    #[test]
    fn conditions_and_remembered_answers_survive_a_json_round_trip() {
        let mut store = store_of(vec![("A", vec![cols(2)])]);
        store.remember_answer("kind", "several-blocks");
        let json = serde_json::to_string(&store).unwrap();
        let back = RecipeStore::from_json(&json).unwrap();
        assert_eq!(back.conditions("A"), &[cols(2)]);
        assert!(back.is_remembered("kind", "several-blocks"));
    }

    #[test]
    fn the_template_with_more_conditions_wins_and_a_tie_is_reported() {
        let ext = Condition::Extension {
            exts: vec!["csv".into()],
        };
        let name = Condition::FileName { text: "uv".into() };
        let store = store_of(vec![
            ("broad", vec![ext.clone()]),
            ("specific", vec![ext.clone(), name.clone()]),
            ("twin", vec![name.clone(), cols(2)]),
        ]);
        let found = store.matching(&facts("uv.csv", Some(2))).unwrap();
        assert_eq!(found.recipe.name, "specific");
        assert_eq!(found.met, 2);
        let tied: Vec<&str> = found.tied.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(tied, ["twin"]);
        // Only the broad one is met: it wins alone.
        let found = store.matching(&facts("other.csv", Some(5))).unwrap();
        assert_eq!(found.recipe.name, "broad");
        assert!(found.tied.is_empty());
    }

    #[test]
    fn a_remembered_answer_is_per_kind_and_forgettable() {
        let mut store = RecipeStore::default();
        store.remember_answer("jasco", "text-rows-in-data");
        store.remember_answer("jasco", "several-blocks");
        assert!(store.is_remembered("jasco", "several-blocks"));
        assert!(!store.is_remembered("ocean", "several-blocks"));
        assert!(!store.is_remembered("jasco", "decimal-or-separator"));
        assert_eq!(store.remembered_answers().len(), 2);
        assert!(store.forget_answer("jasco", "several-blocks"));
        assert!(!store.forget_answer("jasco", "several-blocks"));
        assert!(store.is_remembered("jasco", "text-rows-in-data"));
        assert!(store.forget_answer("jasco", "text-rows-in-data"));
        assert!(store.remembered_answers().is_empty());
    }

    const IR_WITH_NMR: &str = "##TITLE=ir\n##JCAMP-DX=4.24\n##DATA TYPE=INFRARED SPECTRUM\n##XUNITS=1/CM\n##YUNITS=ABSORBANCE\n##FIRSTX=400\n##LASTX=403\n##NPOINTS=4\n##XFACTOR=1\n##YFACTOR=1\n##XYDATA=(X++(Y..Y))\n400 1 2 3 4\n##END=\n##TITLE=nmr\n##JCAMP-DX=4.24\n##DATA TYPE=NMR SPECTRUM\n##FIRSTX=0\n##LASTX=3\n##XYDATA=(X++(Y..Y))\n0 9 9 9 9\n##END=\n";

    #[test]
    fn a_remembered_answer_silences_the_same_doubt_for_the_same_kind_only() {
        let path = Path::new("m.jdx");
        let bytes = IR_WITH_NMR.as_bytes();
        let empty = RecipeStore::default();
        let first = recipe_for_file(&empty, path, bytes, None).unwrap();
        assert_eq!(first.origin, RecipeOrigin::Detected);
        let doubt = first.doubts.first().expect("NMR left out raises a doubt");
        let kind = first.kind.clone().expect("a detected file has a kind");

        let mut store = RecipeStore::default();
        store.remember_answer(&kind, doubt.id());
        let again = recipe_for_file(&store, path, bytes, None).unwrap();
        assert!(again.doubts.is_empty());

        let mut other = RecipeStore::default();
        other.remember_answer("some other kind", doubt.id());
        let other = recipe_for_file(&other, path, bytes, None).unwrap();
        assert_eq!(other.doubts, first.doubts);
    }

    #[test]
    fn the_entry_point_picks_named_then_matching_then_detector() {
        let (path, bytes) = fixture("jasco_uvvis_semicolon.csv");
        let store = store_of(vec![(
            "mine",
            vec![Condition::FileStart {
                text: "origin;jasco".into(),
            }],
        )]);
        let matched = recipe_for_file(&store, &path, &bytes, None).unwrap();
        assert_eq!(
            matched.origin,
            RecipeOrigin::Matched {
                name: "mine".into(),
                tied: vec![]
            }
        );
        assert_eq!(matched.recipe.name, "mine");
        let named = recipe_for_file(&store, &path, &bytes, Some("mine")).unwrap();
        assert_eq!(named.origin, RecipeOrigin::Named("mine".into()));
        assert_eq!(
            recipe_for_file(&store, &path, &bytes, Some("nope")).unwrap_err(),
            ChooseError::UnknownTemplate {
                name: "nope".into()
            }
        );
        let detected = recipe_for_file(&RecipeStore::default(), &path, &bytes, None).unwrap();
        assert_eq!(detected.origin, RecipeOrigin::Detected);
        assert!(detected.kind.is_some());
    }

    #[test]
    fn nmr_is_refused_unless_a_template_is_named() {
        let nmr = b"##TITLE=s\n##JCAMP-DX=5\n##DATA TYPE=NMR SPECTRUM\n##FIRSTX=0\n##LASTX=3\n##NPOINTS=4\n##XYDATA=(X++(Y..Y))\n0 1 2 3 4\n##END=\n";
        let err =
            recipe_for_file(&RecipeStore::default(), Path::new("n.jdx"), nmr, None).unwrap_err();
        assert_eq!(err, ChooseError::Nmr);
        assert_eq!(err.to_string(), "NMR data is not supported");
    }
}

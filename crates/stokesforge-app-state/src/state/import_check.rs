//! What the import screen understood about a file, sorted by how much it can
//! be trusted, in plain sentences, and what the user decided about it.
//!
//! The detector (`stokesforge_import::sniff`) proposes a whole configuration.
//! This module sorts what it proposes:
//!
//! * [`Confidence::Sure`]: stated in a compact line, nothing to do.
//! * [`Confidence::Check`]: a real doubt, asked as a question with its
//!   evidence (the review, one doubt at a time).
//! * [`Confidence::Missing`]: nothing in the file gives it (a unit): the grid
//!   is the fallback.
//!
//! An answer (checked, marked as checked, corrected) is **remembered** for the
//! kind of file in the template store (`RecipeStore::remember_answer`): the
//! same doubt is not raised again for a later file of that kind. The doubt ids
//! of the findings are [`CheckKind::doubt_id`]; those of the detector are
//! `Doubt::id`.
//!
//! Confidence rules (cheap heuristics, all computed from the cached
//! [`FileMap`], never from the bytes):
//!
//! | kind | Sure when | Check when |
//! |---|---|---|
//! | x' line | decided by you or a template | otherwise (an empty first cell followed by numbers also looks like a data row with a hole) |
//! | names + x' on one line | decided | otherwise |
//! | name of the curves (`TITLE`) | at most 3 Y columns, or decided | more than 3 Y columns (one name for 240 curves) |
//! | points (`NPOINTS`) | declared == read | mismatch |
//! | decimal | some fractional token has a number of digits other than 3 | comma decimal, and every fractional token of the first 50 data rows has exactly 3 digits (it may group thousands) |
//! | separator | at least 95 % of the data rows have the same field count | otherwise |
//! | units | read from a key or a units line | `Missing` when nothing gives them |
//! | encoding, data, names, sheet name, footer | always | never |
//!
//! The recognised instrument ([`Fingerprint`]) is computed once per file from
//! the lines of the file (no line count, no full key set in its identifier: the
//! header length and the optional keys vary between files of one instrument).

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use stokesforge_import::props::{norm_key, prop_text, split_kv, split_name_unit};
use stokesforge_import::sniff::{sniff, Family, SepKind, Sniff, SniffRefusal};
use stokesforge_import::{
    file_map, DecimalStyle, Encoding, FileFacts, FileMap, ImportRecipe, LineRole, NameSource,
    ParsedTable, PropField, PropTarget,
};

use super::AppState;
use crate::tr;

// -- Types -------------------------------------------------------------------

/// How much a finding can be trusted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Confidence {
    Sure,
    Check,
    Missing,
}

/// Where a certainty comes from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Source {
    File,
    Template(String),
    You,
}

/// The things the import screen has an opinion about.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CheckKind {
    Separator,
    Decimal,
    Encoding,
    Data,
    Points,
    XPrime,
    Names,
    NameXPrime,
    XUnit,
    YUnit,
    XPrimeUnit,
    YName,
    SheetName,
    Footer,
    /// Comments read from a key of the header.
    Comment,
}

impl CheckKind {
    /// Every kind, in declaration order.
    pub const ALL: [CheckKind; 15] = [
        CheckKind::Separator,
        CheckKind::Decimal,
        CheckKind::Encoding,
        CheckKind::Data,
        CheckKind::Points,
        CheckKind::XPrime,
        CheckKind::Names,
        CheckKind::NameXPrime,
        CheckKind::XUnit,
        CheckKind::YUnit,
        CheckKind::XPrimeUnit,
        CheckKind::YName,
        CheckKind::SheetName,
        CheckKind::Footer,
        CheckKind::Comment,
    ];

    /// Stable id of the doubt this kind raises, the key of the answers
    /// remembered per kind of file (distinct from the detector's `Doubt::id`).
    pub fn doubt_id(self) -> &'static str {
        match self {
            CheckKind::Separator => "check-separator",
            CheckKind::Decimal => "check-decimal",
            CheckKind::Encoding => "check-encoding",
            CheckKind::Data => "check-data",
            CheckKind::Points => "check-points",
            CheckKind::XPrime => "check-x-prime",
            CheckKind::Names => "check-names",
            CheckKind::NameXPrime => "check-names-x-prime",
            CheckKind::XUnit => "check-x-unit",
            CheckKind::YUnit => "check-y-unit",
            CheckKind::XPrimeUnit => "check-x-prime-unit",
            CheckKind::YName => "check-y-name",
            CheckKind::SheetName => "check-sheet-name",
            CheckKind::Footer => "check-footer",
            CheckKind::Comment => "check-comment",
        }
    }
}

/// A place in the file: a 1-based line and, optionally, a cell of it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Evidence {
    pub line: usize,
    pub cell: Option<usize>,
}

/// A chip inside a sentence: what its popup edits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChipId {
    Separator,
    Decimal,
    Encoding,
    DataFirst,
    DataLast,
    /// The key that gives a unit or a name.
    Key(CheckKind),
}

/// A piece of a sentence.
#[derive(Debug, Clone, PartialEq)]
pub enum Piece {
    Text(String),
    Chip { text: String, id: ChipId },
}

/// What a button of a question does to the recipe.
#[derive(Debug, Clone, PartialEq)]
pub enum CheckAction {
    /// Keep as read.
    Confirm(CheckKind),
    /// The x' line is data: no x' role, the data start there (1-based line).
    XPrimeAsData { line: usize },
    /// The line gives the names only: no x' role.
    NamesOnly { line: usize },
    /// The line is data: no role, the data start there.
    LineAsData { line: usize, kind: CheckKind },
    /// Only the sheet takes the name of this key.
    NameForSheetOnly { key: String },
    /// No name from this key at all.
    NoName { key: String },
    /// The comma groups thousands.
    CommaGroupsThousands,
    /// Let the user click a line of the file.
    PickLine(CheckKind),
    /// Take the units of this key (X or Y) instead of the other source.
    UnitFromKey { kind: CheckKind, key: String },
}

/// One answer of a question.
#[derive(Debug, Clone, PartialEq)]
pub struct Answer {
    pub label: String,
    pub action: CheckAction,
}

/// The question a doubt asks, with its evidence.
#[derive(Debug, Clone, PartialEq)]
pub struct Question {
    pub title: String,
    pub explain: String,
    pub support: Vec<(Evidence, String)>,
    /// The first answer is the recommended one.
    pub answers: Vec<Answer>,
}

/// One thing the import screen understood about the file.
#[derive(Debug, Clone, PartialEq)]
pub struct CheckFinding {
    pub kind: CheckKind,
    pub confidence: Confidence,
    pub source: Source,
    pub pieces: Vec<Piece>,
    pub detail: String,
    pub evidence: Vec<Evidence>,
    pub question: Option<Question>,
    /// The key of the file the value comes from (units, names).
    pub key: Option<String>,
}

/// How the instrument was recognised.
#[derive(Debug, Clone, PartialEq)]
pub struct Fingerprint {
    pub recognised: bool,
    pub vendor: String,
    pub model: Option<String>,
    pub data_type: Option<String>,
    pub marker: Option<String>,
    pub sep: SepKind,
    pub comma: bool,
    pub curves: usize,
    pub kinetic: bool,
    pub evidence: Vec<(Evidence, String)>,
    pub id: String,
    /// « JASCO V-770 · UV spectrum · kinetic (240 curves) ».
    pub display: String,
    /// « JASCO V-770 · UV kinetic ».
    pub template_name: String,
    /// What the template conditions look at in this file (origin, data type,
    /// marker, numbers, kinetic, number of columns, header keys...).
    pub facts: FileFacts,
}

/// A key of the file that could give a unit or a name.
#[derive(Debug, Clone, PartialEq)]
pub struct KeyEntry {
    pub key: String,
    pub line: usize,
    pub value: String,
}

/// Everything the left column needs, built once per (file, recipe, decisions).
#[derive(Debug, Clone)]
pub struct CheckView {
    pub fingerprint: Arc<Fingerprint>,
    /// Order: Check, Missing, Sure.
    pub findings: Vec<CheckFinding>,
    pub n_check: usize,
    pub n_missing: usize,
    pub n_sure: usize,
    /// The template the file is read with, if any.
    pub template: Option<String>,
    pub receipt: String,
    pub keys: Vec<KeyEntry>,
    /// Why a decision of the template was taken back.
    pub note: Option<String>,
    /// First and last data line, 1-based.
    pub data: Option<(usize, usize)>,
    /// « What was found », in display order.
    pub rows: Vec<FoundRow>,
    /// Number of data lines and of cells per data line.
    pub n_rows: usize,
    pub n_cols: usize,
    /// A decimal number of the data, as written (« 0,000522105 »).
    pub decimal_sample: String,
    /// Why the text encoding was settled (« ° » on line 603), if it says.
    pub encoding_detail: String,
    /// Name of the encoding read.
    pub encoding_name: String,
    /// What the detector found for the four format settings (« Auto (; ) »).
    pub detected: DetectedFormat,
}

/// The format settings the detector found.
#[derive(Debug, Clone, PartialEq)]
pub struct DetectedFormat {
    pub separator: stokesforge_import::Separator,
    pub decimal: DecimalStyle,
    pub thousands: Option<char>,
    pub encoding: Encoding,
}

/// The groups of « What was found ».
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FoundGroup {
    Data,
    Columns,
    Sheet,
}

/// The role colour of a found value, if it is a role value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FoundRole {
    None,
    LongName,
    Units,
    Comments,
    XPrime,
}

/// One row of « What was found »: a label, the chosen value, where it comes
/// from.
#[derive(Debug, Clone, PartialEq)]
pub struct FoundRow {
    pub kind: CheckKind,
    pub group: FoundGroup,
    pub label: String,
    pub value: String,
    pub role: FoundRole,
    pub source: String,
    pub confidence: Confidence,
    pub evidence: Vec<Evidence>,
    /// The popup that corrects the value, if it has one.
    pub chip: Option<ChipId>,
}

impl CheckView {
    pub fn finding(&self, kind: CheckKind) -> Option<&CheckFinding> {
        self.findings.iter().find(|f| f.kind == kind)
    }

    /// The doubts, in review order.
    pub fn doubts(&self) -> Vec<&CheckFinding> {
        self.findings
            .iter()
            .filter(|f| f.confidence == Confidence::Check)
            .collect()
    }
}

/// The review in progress: the doubts, in order.
#[derive(Debug, Clone, Default)]
pub struct Review {
    pub order: Vec<CheckKind>,
    pub index: usize,
    /// What the user decided, as sentences.
    pub log: Vec<String>,
}

/// The review as the panel shows it.
#[derive(Debug, Clone)]
pub struct ReviewInfo {
    pub index: usize,
    pub total: usize,
    pub finding: CheckFinding,
}

/// What the user waits for after a click on a line of the file.
pub type Pick = CheckKind;

/// What the left panel asks of the check layer, per import window.
#[derive(Default)]
pub struct ImportChecks {
    /// Bumped at each answer, so that the cached view is rebuilt.
    pub generation: u64,
    fingerprints: RefCell<HashMap<PathBuf, Arc<Fingerprint>>>,
    cache: RefCell<Option<(CacheKey, Arc<CheckView>)>>,
    pub review: Option<Review>,
    /// The review that has just ended: file and what was decided.
    pub review_done: Option<(PathBuf, Vec<String>)>,
    pub pick: Option<Pick>,
    /// Instruments whose offer to save a template was declined.
    pub offer_dismissed: HashSet<String>,
    /// Template each file is read with.
    pub template_of_file: HashMap<PathBuf, String>,
    /// The other templates that meet as many conditions as the one chosen.
    pub tied_of_file: HashMap<PathBuf, Vec<String>>,
    /// Files the user asked to read with the plain detection.
    pub auto_paths: HashSet<PathBuf>,
    /// The "Save as Template" window, while open.
    pub save_draft: Option<super::import_template::SaveDraft>,
    /// The "Manage Templates" window, while open.
    pub manage: Option<super::import_template::ManageState>,
    /// The template whose "Forget Template…" awaits the confirmation.
    pub forget_pending: Option<String>,
}

type CacheKey = (PathBuf, ImportRecipe, u64);

impl ImportChecks {
    /// The fingerprint of a file, computed once.
    pub fn fingerprint(
        &self,
        path: &Path,
        bytes: &[u8],
        sniffed: &Sniff,
    ) -> Option<Arc<Fingerprint>> {
        if let Some(found) = self.fingerprints.borrow().get(path) {
            return Some(found.clone());
        }
        let map = file_map(bytes, &sniffed.recipe).ok()?;
        let facts = FileFacts::new(path, bytes, Some(sniffed));
        let fp = Arc::new(compute_fingerprint(&map, sniffed, facts));
        self.fingerprints
            .borrow_mut()
            .insert(path.to_path_buf(), fp.clone());
        Some(fp)
    }

    /// The identifier of the fingerprint already computed for a file.
    pub(super) fn cached_fingerprint_id(&self, path: &Path) -> Option<String> {
        self.fingerprints.borrow().get(path).map(|fp| fp.id.clone())
    }

    fn cached_fingerprint(&self, path: &Path) -> Option<Arc<Fingerprint>> {
        self.fingerprints.borrow().get(path).cloned()
    }

    /// Forgets what was computed for a file that left the window.
    pub(super) fn forget_file(&mut self, path: &Path) {
        self.fingerprints.borrow_mut().remove(path);
        self.template_of_file.remove(path);
        self.tied_of_file.remove(path);
        self.auto_paths.remove(path);
        self.generation += 1;
    }
}

// -- Small helpers ----------------------------------------------------------

/// Markers that stand for chips inside a translated sentence.
const MARK: &str = "\u{1}";

fn marker(index: usize) -> String {
    format!("{MARK}{index}{MARK}")
}

fn pieces_from(sentence: &str, chips: Vec<(String, ChipId)>) -> Vec<Piece> {
    let mut out = Vec::new();
    for (i, part) in sentence.split(MARK).enumerate() {
        if i % 2 == 0 {
            if !part.is_empty() {
                out.push(Piece::Text(part.to_string()));
            }
        } else if let Some((text, id)) = part.parse::<usize>().ok().and_then(|n| chips.get(n)) {
            out.push(Piece::Chip {
                text: text.clone(),
                id: *id,
            });
        }
    }
    out
}

fn num_token(token: &str, comma: bool) -> Option<f64> {
    let t = token.trim();
    if t.is_empty() {
        return None;
    }
    let value: f64 = if comma {
        t.replace(',', ".").parse().ok()?
    } else {
        t.parse().ok()?
    };
    value.is_finite().then_some(value)
}

fn fmt_num(value: f64) -> String {
    if value.fract().abs() < 1e-9 {
        format!("{value:.0}")
    } else {
        let text = format!("{value:.3}");
        text.trim_end_matches('0').trim_end_matches('.').to_string()
    }
}

pub(super) fn short_data_type(raw: &str) -> String {
    let upper = raw.trim().to_uppercase();
    if upper.contains("ULTRAVIOLET") {
        tr!("import-datatype-uv")
    } else if upper.contains("FLUORESCENCE") {
        tr!("import-datatype-fluo")
    } else if upper.contains("INFRARED") {
        tr!("import-datatype-ir")
    } else {
        raw.trim().to_lowercase()
    }
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

pub(super) fn sep_name(kind: SepKind) -> String {
    match kind {
        SepKind::Semicolon => tr!("import-sepname-semicolon"),
        SepKind::Tab => tr!("import-sepname-tab"),
        SepKind::Comma => tr!("import-sepname-comma"),
        SepKind::Whitespace => tr!("import-sepname-space"),
        SepKind::None => tr!("import-sepname-none"),
    }
}

fn sep_kind_of(map: &FileMap) -> SepKind {
    match map.separator_char() {
        Some(';') => SepKind::Semicolon,
        Some('\t') => SepKind::Tab,
        Some(',') => SepKind::Comma,
        Some(_) => SepKind::Whitespace,
        None => SepKind::Whitespace,
    }
}

// -- The fingerprint ---------------------------------------------------------

fn compute_fingerprint(map: &FileMap, sniff: &Sniff, facts: FileFacts) -> Fingerprint {
    let n = map.line_count();
    let mut origin: Option<Evidence> = None;
    let mut model: Option<(String, Evidence)> = None;
    let mut data_type: Option<(String, Evidence)> = None;
    let mut footer_model: Option<(String, Evidence)> = None;
    let scan_to = n.min(2000);
    for i in map.first..scan_to {
        let Some((key, value)) = split_kv(map.line(i)) else {
            continue;
        };
        let ev = Evidence {
            line: i + 1,
            cell: Some(1),
        };
        match norm_key(key).as_str() {
            "ORIGIN" if value.eq_ignore_ascii_case("JASCO") && origin.is_none() => {
                origin = Some(ev);
            }
            "SPECTROMETER/DATA SYSTEM" if model.is_none() => {
                model = model_token(value).map(|m| (m, ev));
            }
            "DATA TYPE" if data_type.is_none() && !value.is_empty() => {
                data_type = Some((value.to_string(), ev));
            }
            "MODEL NAME" if footer_model.is_none() && !value.is_empty() => {
                footer_model = Some((value.to_string(), ev));
            }
            _ => {}
        }
    }
    let model = model.or(footer_model);
    let jasco = origin.is_some() || sniff.family == Some(Family::Jasco);
    let jcamp = sniff.family == Some(Family::Jcamp);
    let marker = sniff.recipe.main_until_marker.clone();
    let columns = sniff.columns;
    let has_xprime = sniff.recipe.subheader_roles.x_prime.is_some();
    let kinetic = has_xprime && columns > 2;
    let curves = columns.saturating_sub(1);
    let comma = sniff.kind.comma;
    let sep = sniff.kind.sep;
    let vendor = if jasco {
        tr!("import-auto-family-jasco")
    } else if jcamp {
        tr!("import-auto-family-jcamp")
    } else {
        String::new()
    };
    let recognised = jasco || jcamp;
    let dt_short = data_type.as_ref().map(|(raw, _)| short_data_type(raw));
    let kinetic_text = || tr!("import-kinetic", count = curves.to_string());
    let display = if recognised {
        let mut parts: Vec<String> = Vec::new();
        parts.push(match &model {
            Some((m, _)) => format!("{vendor} {m}"),
            None => vendor.clone(),
        });
        if let Some(dt) = &dt_short {
            parts.push(dt.clone());
        }
        if kinetic {
            parts.push(kinetic_text());
        }
        parts.join(" · ")
    } else if has_xprime && columns > 2 {
        tr!("import-shape-matrix", count = curves.to_string())
    } else if sniff.recipe.has_column_header_row {
        tr!("import-shape-named", count = columns.to_string())
    } else {
        tr!("import-shape-plain", count = columns.to_string())
    };
    let template_name = if recognised {
        let base = match &model {
            Some((m, _)) => format!("{vendor} {m}"),
            None => vendor.clone(),
        };
        let kind = dt_short
            .as_ref()
            .map(|d| d.replace(" spectrum", "").replace(" spectre", ""))
            .unwrap_or_default();
        let tail = match (kind.is_empty(), kinetic) {
            (true, false) => String::new(),
            (true, true) => format!(" · {}", tr!("import-template-name-kinetic")),
            (false, false) => format!(" · {kind}"),
            (false, true) => format!(" · {kind} {}", tr!("import-template-name-kinetic")),
        };
        format!("{base}{tail}")
    } else {
        display.clone()
    };
    let mut evidence: Vec<(Evidence, String)> = Vec::new();
    if let Some(ev) = origin {
        if let Some(line) = Some(map.line(ev.line - 1)) {
            evidence.push((ev, line.trim().to_string()));
        }
    }
    if let Some((m, ev)) = &model {
        evidence.push((*ev, m.clone()));
    }
    if let Some((raw, ev)) = &data_type {
        evidence.push((*ev, raw.trim().to_string()));
    }
    if let Some(m) = &marker {
        if let Some(i) = (map.first..n).find(|&i| map.line(i).trim().eq_ignore_ascii_case(m)) {
            evidence.push((
                Evidence {
                    line: i + 1,
                    cell: Some(0),
                },
                m.clone(),
            ));
        }
    }
    if jcamp && origin.is_none() {
        if let Some(i) =
            (map.first..n.min(map.first + 80)).find(|&i| map.line(i).trim_start().starts_with("##"))
        {
            evidence.push((
                Evidence {
                    line: i + 1,
                    cell: Some(0),
                },
                map.line(i).trim().to_string(),
            ));
        }
    }
    if let Some((a, _)) = map.data_range {
        evidence.push((
            Evidence {
                line: a + 1,
                cell: None,
            },
            tr!("import-fingerprint-columns", count = columns.to_string()),
        ));
    }
    let id = format!(
        "{}|{}|{}|{}|{:?}|{}|{}",
        vendor,
        model.as_ref().map_or("", |(m, _)| m.as_str()),
        data_type
            .as_ref()
            .map_or(String::new(), |(d, _)| d.trim().to_uppercase()),
        marker.clone().unwrap_or_default().to_uppercase(),
        sep,
        comma,
        kinetic
    );
    Fingerprint {
        recognised,
        vendor,
        model: model.map(|(m, _)| m),
        data_type: data_type.map(|(d, _)| d),
        marker,
        sep,
        comma,
        curves,
        kinetic,
        evidence,
        id,
        display,
        template_name,
        facts,
    }
}

// -- Does a dropped file need a look? ----------------------------------------

/// Does a dropped file, read with the recipe `chosen` for it, still have a
/// point to check? A file a template reads is decided (unless two templates
/// tie); otherwise the detector's own doubts and the doubts of the check view
/// count, each one after the answers already remembered for the kind of file.
pub(super) fn drop_needs_a_look(
    path: &Path,
    bytes: &[u8],
    chosen: &stokesforge_import::ChosenRecipe,
    store: &stokesforge_import::RecipeStore,
) -> bool {
    use stokesforge_import::RecipeOrigin;
    if !chosen.doubts.is_empty() {
        return true;
    }
    let kind = chosen.kind.as_deref().unwrap_or_default();
    match &chosen.origin {
        RecipeOrigin::Named(_) => false,
        RecipeOrigin::Matched { name, tied } => {
            !tied.is_empty()
                && !store.is_remembered(kind, &super::import_tocheck::tie_id(name, tied))
        }
        RecipeOrigin::Detected => {
            let Some(sniffed) = chosen.sniffed.as_ref() else {
                return true;
            };
            let Ok(map) = file_map(bytes, &chosen.recipe) else {
                return true;
            };
            let facts = FileFacts::new(path, bytes, Some(sniffed));
            let fingerprint = Arc::new(compute_fingerprint(&map, sniffed, facts));
            let decided: HashMap<CheckKind, Source> = CheckKind::ALL
                .into_iter()
                .filter(|k| store.is_remembered(kind, k.doubt_id()))
                .map(|k| (k, Source::You))
                .collect();
            let view = build_view(
                &map,
                sniffed,
                &chosen.recipe,
                Some(&decided),
                fingerprint,
                None,
                None,
                String::new(),
            );
            !view.doubts().is_empty()
        }
    }
}

// -- The findings -----------------------------------------------------------

struct Ctx<'a> {
    map: &'a FileMap,
    recipe: &'a ImportRecipe,
    decided: Option<&'a HashMap<CheckKind, Source>>,
    n_y: usize,
}

impl Ctx<'_> {
    fn decision(&self, kind: CheckKind) -> Option<Source> {
        self.decided.and_then(|d| d.get(&kind).cloned())
    }
}

fn first_x_prime_line(map: &FileMap) -> Option<usize> {
    (map.first..map.line_count()).find(|&i| map.subheader_roles.get(i).is_some_and(|r| r.x_prime))
}

/// A file has an x' line (the times of a kinetic) only when it has more than
/// this many columns; a short table never gets the x' doubt nor the missing
/// x' lines.
const X_PRIME_MIN_COLUMNS: usize = 5;

/// Does the x' reading make sense for a file of `n_cols` columns?
fn x_prime_applies(n_cols: usize) -> bool {
    n_cols > X_PRIME_MIN_COLUMNS
}

fn long_name_line(map: &FileMap) -> Option<usize> {
    (map.first..map.line_count()).find(|&i| map.subheader_roles.get(i).is_some_and(|r| r.long_name))
}

fn unit_line(map: &FileMap) -> Option<usize> {
    (map.first..map.line_count()).find(|&i| map.subheader_roles.get(i).is_some_and(|r| r.unit))
}

/// Does the line look like an x' line: an empty first cell, then numbers?
fn looks_like_x_prime(
    map: &FileMap,
    recipe: &ImportRecipe,
    line_index: usize,
    comma: bool,
) -> bool {
    let tokens = map.tokens(line_index, recipe);
    tokens.len() >= 3
        && tokens[0].trim().is_empty()
        && tokens[1..]
            .iter()
            .filter(|t| !t.trim().is_empty())
            .all(|t| num_token(t, comma).is_some())
}

/// Numeric values of the cells after the first one of a line.
fn row_numbers(map: &FileMap, recipe: &ImportRecipe, line_index: usize, comma: bool) -> Vec<f64> {
    map.tokens(line_index, recipe)
        .iter()
        .skip(1)
        .filter_map(|t| num_token(t, comma))
        .collect()
}

/// `KEY` lines of the header and the footer: first line of each key.
fn key_index(map: &FileMap) -> Vec<KeyEntry> {
    let mut seen: HashSet<String> = HashSet::new();
    let mut out = Vec::new();
    for i in 0..map.line_count() {
        if map.roles.get(i) == Some(&LineRole::Data) {
            continue;
        }
        let Some((key, value)) = split_kv(map.line(i)) else {
            continue;
        };
        if value.is_empty() || !key.chars().any(char::is_alphabetic) {
            continue;
        }
        if seen.insert(norm_key(key)) {
            out.push(KeyEntry {
                key: key.to_string(),
                line: i + 1,
                value: value.to_string(),
            });
        }
    }
    out
}

fn value_cell(map: &FileMap, recipe: &ImportRecipe, line_index: usize) -> usize {
    if map.separator_char().is_some() && map.tokens(line_index, recipe).len() >= 2 {
        1
    } else {
        0
    }
}

/// Where a unit or a name comes from.
struct Origin {
    text: String,
    key: Option<String>,
    line: usize,
    cell: usize,
}

fn prop_origin(ctx: &Ctx, keys: &[KeyEntry], field: PropField, y: bool) -> Option<Origin> {
    let prop = ctx.recipe.header_props.iter().find(|p| {
        p.field == field
            && match p.target {
                PropTarget::X => !y,
                PropTarget::Y => y,
                PropTarget::All => true,
            }
    })?;
    let wanted = norm_key(&prop.key);
    let entry = keys.iter().find(|k| norm_key(&k.key) == wanted)?;
    let text = prop_text(prop, &entry.value)?;
    Some(Origin {
        text,
        key: Some(entry.key.clone()),
        line: entry.line,
        cell: value_cell(ctx.map, ctx.recipe, entry.line - 1),
    })
}

fn units_line_origin(ctx: &Ctx, y: bool) -> Option<Origin> {
    let i = unit_line(ctx.map)?;
    let tokens = ctx.map.tokens(i, ctx.recipe);
    let column = usize::from(y);
    let text = tokens.get(column)?.trim().to_string();
    (!text.is_empty()).then_some(Origin {
        text,
        key: None,
        line: i + 1,
        cell: column,
    })
}

fn source_or_file(ctx: &Ctx, kind: CheckKind) -> Source {
    ctx.decision(kind).unwrap_or(Source::File)
}

#[allow(clippy::too_many_arguments)]
fn finding(
    kind: CheckKind,
    confidence: Confidence,
    source: Source,
    sentence: String,
    chips: Vec<(String, ChipId)>,
    detail: String,
    evidence: Vec<Evidence>,
    question: Option<Question>,
) -> CheckFinding {
    CheckFinding {
        kind,
        confidence,
        source,
        pieces: pieces_from(&sentence, chips),
        detail,
        evidence,
        question,
        key: None,
    }
}

#[allow(clippy::too_many_arguments)]
fn found(
    rows: &mut Vec<FoundRow>,
    kind: CheckKind,
    group: FoundGroup,
    label: String,
    value: String,
    role: FoundRole,
    source: String,
    confidence: Confidence,
    evidence: Vec<Evidence>,
    chip: Option<ChipId>,
) {
    rows.push(FoundRow {
        kind,
        group,
        label,
        value,
        role,
        source,
        confidence,
        evidence,
        chip,
    });
}

/// Everything about a file that the left column shows. `encoding` is the
/// encoding found by the detector, `receipt` the sentence of the result.
#[allow(clippy::too_many_arguments)]
pub fn build_view(
    map: &FileMap,
    sniff_result: &Sniff,
    recipe: &ImportRecipe,
    decided: Option<&HashMap<CheckKind, Source>>,
    fingerprint: Arc<Fingerprint>,
    template: Option<String>,
    encoding: Option<&str>,
    receipt: String,
) -> CheckView {
    let keys = key_index(map);
    let comma = map.decimal_is_comma();
    let data = map.data_range;
    // A JCAMP file is read by its own reader (#488): x and y only, its
    // points decoded from the table, not counted as lines of the grid.
    let jcamp = recipe.format == stokesforge_import::FileFormat::Jcamp;
    let n_cols = if jcamp {
        2
    } else {
        data.map(|(a, _)| map.tokens(a, recipe).len())
            .unwrap_or(sniff_result.columns)
    };
    let ctx = Ctx {
        map,
        recipe,
        decided,
        n_y: n_cols.saturating_sub(1).max(1),
    };
    let mut note: Option<String> = None;
    let mut out: Vec<CheckFinding> = Vec::new();
    let mut rows: Vec<FoundRow> = Vec::new();
    let encoding_name: String;
    let encoding_detail: String;
    let decimal_sample: String;
    let n_rows = data.map_or(0, |(a, b)| b + 1 - a);
    let data_ev = |cell: Option<usize>| {
        data.map(|(a, _)| Evidence { line: a + 1, cell })
            .into_iter()
            .collect::<Vec<_>>()
    };

    // Encoding.
    {
        let name = match recipe.encoding {
            Encoding::Auto => encoding.unwrap_or("UTF-8").to_string(),
            Encoding::Utf8 => "UTF-8".to_string(),
            Encoding::Windows1252 => "Windows-1252".to_string(),
            Encoding::Latin1 => "Latin-1".to_string(),
            Encoding::Utf16Le => "UTF-16 LE".to_string(),
            Encoding::Utf16Be => "UTF-16 BE".to_string(),
        };
        let non_ascii = (map.first..map.line_count())
            .find(|&i| !map.line(i).is_ascii())
            .map(|i| {
                let ch = map.line(i).chars().find(|c| !c.is_ascii()).unwrap_or('?');
                (i + 1, ch)
            });
        let detail = non_ascii.map_or(
            tr!("import-help-encoding-plain", name = name.clone()),
            |(line, ch)| {
                tr!(
                    "import-help-encoding",
                    ch = ch.to_string(),
                    line = line.to_string(),
                    name = name.clone()
                )
            },
        );
        encoding_name = name.clone();
        encoding_detail = detail.clone();
        out.push(finding(
            CheckKind::Encoding,
            Confidence::Sure,
            source_or_file(&ctx, CheckKind::Encoding),
            tr!("import-check-encoding", c0 = marker(0)),
            vec![(name, ChipId::Encoding)],
            detail,
            non_ascii
                .map(|(line, _)| Evidence { line, cell: None })
                .into_iter()
                .collect(),
            None,
        ));
    }

    // Separator.
    {
        let mut counts: HashMap<usize, usize> = HashMap::new();
        let mut total = 0usize;
        if let Some((a, b)) = data {
            for i in a..=b.min(a + 300) {
                *counts.entry(map.tokens(i, recipe).len()).or_default() += 1;
                total += 1;
            }
        }
        let best = counts.values().copied().max().unwrap_or(0);
        let steady = jcamp || total == 0 || best as f64 >= 0.95 * total as f64;
        let decision = ctx.decision(CheckKind::Separator);
        let confidence = if steady || decision.is_some() {
            Confidence::Sure
        } else {
            Confidence::Check
        };
        let label = {
            let kind = sep_kind_of(map);
            if map.is_whitespace_separated() {
                sep_name(SepKind::Whitespace)
            } else {
                sep_name(kind)
            }
        };
        let detail = if steady {
            String::new()
        } else {
            tr!("import-check-separator-irregular")
        };
        out.push(finding(
            CheckKind::Separator,
            confidence,
            decision.unwrap_or(Source::File),
            tr!("import-check-separator", c0 = marker(0)),
            vec![(label, ChipId::Separator)],
            detail,
            data_ev(None),
            None,
        ));
    }

    // Decimal.
    {
        let mut sample = String::new();
        let mut sample_cell = 0usize;
        let mut varied = false;
        let mut seen_fraction = false;
        let point = if comma { ',' } else { '.' };
        if let Some((a, b)) = data {
            'rows: for i in a..=b.min(a + 49) {
                for (c, t) in map.tokens(i, recipe).iter().take(12).enumerate() {
                    let t = t.trim();
                    let Some((int, frac)) = t.split_once(point) else {
                        continue;
                    };
                    if sample.is_empty()
                        && int
                            .trim_start_matches('-')
                            .chars()
                            .all(|c| c.is_ascii_digit())
                    {
                        sample = t.to_string();
                        sample_cell = c;
                    }
                    let digits = int
                        .trim_start_matches('-')
                        .chars()
                        .all(|c| c.is_ascii_digit())
                        && !frac.is_empty()
                        && frac.chars().all(|c| c.is_ascii_digit());
                    if !digits {
                        continue;
                    }
                    seen_fraction = true;
                    if frac.len() != 3 {
                        varied = true;
                        break 'rows;
                    }
                }
            }
        }
        decimal_sample = sample.clone();
        let ambiguous = comma && seen_fraction && !varied;
        let decision = ctx.decision(CheckKind::Decimal);
        let confidence = if ambiguous && decision.is_none() {
            Confidence::Check
        } else {
            Confidence::Sure
        };
        let label = if comma {
            tr!("import-check-decimal-comma")
        } else {
            tr!("import-check-decimal-point")
        };
        let line = data.map_or(1, |(a, _)| a + 1);
        let question = (confidence == Confidence::Check).then(|| Question {
            title: tr!("import-check-q-decimal-title", sample = sample.clone()),
            explain: tr!("import-check-q-decimal-explain", sample = sample.clone()),
            support: Vec::new(),
            answers: vec![
                Answer {
                    label: tr!("import-check-q-decimal-a1", sample = sample.clone()),
                    action: CheckAction::Confirm(CheckKind::Decimal),
                },
                Answer {
                    label: tr!("import-check-q-decimal-a2"),
                    action: CheckAction::CommaGroupsThousands,
                },
            ],
        });
        out.push(finding(
            CheckKind::Decimal,
            confidence,
            decision.unwrap_or(Source::File),
            if sample.is_empty() {
                tr!("import-check-decimal-plain", c0 = marker(0))
            } else {
                tr!(
                    "import-check-decimal",
                    c0 = marker(0),
                    sample = sample.clone()
                )
            },
            vec![(label, ChipId::Decimal)],
            String::new(),
            vec![Evidence {
                line,
                cell: Some(sample_cell),
            }],
            question,
        ));
    }

    // Data.
    if let Some((a, b)) = data {
        let rows_read = b - a + 1;
        found(
            &mut rows,
            CheckKind::Data,
            FoundGroup::Data,
            tr!("import-found-label-data"),
            tr!(
                "import-found-value-lines",
                first = (a + 1).to_string(),
                last = (b + 1).to_string()
            ),
            FoundRole::None,
            tr!(
                "import-check-data-detail",
                rows = rows_read.to_string(),
                columns = n_cols.to_string()
            ),
            Confidence::Sure,
            vec![
                Evidence {
                    line: a + 1,
                    cell: None,
                },
                Evidence {
                    line: b + 1,
                    cell: None,
                },
            ],
            Some(ChipId::DataFirst),
        );
        out.push(finding(
            CheckKind::Data,
            Confidence::Sure,
            source_or_file(&ctx, CheckKind::Data),
            tr!("import-check-data", c0 = marker(0), c1 = marker(1)),
            vec![
                ((a + 1).to_string(), ChipId::DataFirst),
                ((b + 1).to_string(), ChipId::DataLast),
            ],
            tr!(
                "import-check-data-detail",
                rows = rows_read.to_string(),
                columns = n_cols.to_string()
            ),
            vec![
                Evidence {
                    line: a + 1,
                    cell: None,
                },
                Evidence {
                    line: b + 1,
                    cell: None,
                },
            ],
            None,
        ));
    }

    // Names, x', and names on the x' line.
    let xp_line = first_x_prime_line(map).filter(|_| x_prime_applies(n_cols));
    let ln_line = long_name_line(map);
    if let Some(row) = map.column_header {
        found(
            &mut rows,
            CheckKind::Names,
            FoundGroup::Columns,
            tr!("import-found-label-names"),
            tr!("import-found-value-line", line = (row + 1).to_string()),
            FoundRole::None,
            String::new(),
            Confidence::Sure,
            vec![Evidence {
                line: row + 1,
                cell: Some(0),
            }],
            None,
        );
        out.push(finding(
            CheckKind::Names,
            Confidence::Sure,
            source_or_file(&ctx, CheckKind::Names),
            tr!("import-check-names", line = (row + 1).to_string()),
            vec![],
            String::new(),
            vec![Evidence {
                line: row + 1,
                cell: Some(0),
            }],
            None,
        ));
    }
    if let Some(i) = xp_line {
        let numbers = row_numbers(map, recipe, i, comma);
        let count = numbers.len();
        let (first, last) = (
            numbers.first().copied().unwrap_or(0.0),
            numbers.last().copied().unwrap_or(0.0),
        );
        let grows = numbers.windows(2).all(|w| w[1] >= w[0]) && count > 1;
        let line = i + 1;
        let with_names = ln_line == Some(i);
        let kind = if with_names {
            CheckKind::NameXPrime
        } else {
            CheckKind::XPrime
        };
        let mut decision = ctx.decision(kind);
        // A template says times, but this line looks like data.
        if matches!(decision, Some(Source::Template(_)))
            && !with_names
            && !looks_like_x_prime(map, recipe, i, comma)
        {
            decision = None;
            note = Some(tr!("import-check-note-template-xprime"));
        }
        let mut support: Vec<(Evidence, String)> = Vec::new();
        if grows {
            support.push((
                Evidence { line, cell: None },
                tr!(
                    "import-check-support-grow",
                    first = fmt_num(first),
                    last = fmt_num(last)
                ),
            ));
        }
        // Counts written in the notes that equal the number of x' values.
        let mut counts = 0usize;
        for entry in &keys {
            if counts >= 2 {
                break;
            }
            if entry.value.trim().parse::<usize>().ok() == Some(count) && count > 2 {
                support.push((
                    Evidence {
                        line: entry.line,
                        cell: Some(value_cell(map, recipe, entry.line - 1)),
                    },
                    tr!(
                        "import-check-support-count",
                        key = entry.key.clone(),
                        value = entry.value.trim().to_string()
                    ),
                ));
                counts += 1;
            }
        }
        let main_ev = vec![Evidence {
            line,
            cell: Some(0),
        }];
        {
            let decided_now = decision.is_some();
            found(
                &mut rows,
                kind,
                FoundGroup::Columns,
                tr!("import-found-label-xprime"),
                tr!("import-found-value-line", line = line.to_string()),
                FoundRole::XPrime,
                tr!(
                    "import-check-xprime-detail",
                    count = count.to_string(),
                    first = fmt_num(first),
                    last = fmt_num(last)
                ),
                if decided_now {
                    Confidence::Sure
                } else {
                    Confidence::Check
                },
                main_ev.clone(),
                None,
            );
        }
        if with_names {
            let confidence = if decision.is_some() {
                Confidence::Sure
            } else {
                Confidence::Check
            };
            let question = (confidence == Confidence::Check).then(|| Question {
                title: tr!("import-check-q-namexp-title", line = line.to_string()),
                explain: tr!("import-check-q-namexp-explain", count = count.to_string()),
                support: support.clone(),
                answers: vec![
                    Answer {
                        label: tr!("import-check-q-namexp-a1", line = line.to_string()),
                        action: CheckAction::Confirm(CheckKind::NameXPrime),
                    },
                    Answer {
                        label: tr!("import-check-q-namexp-a2"),
                        action: CheckAction::NamesOnly { line },
                    },
                    Answer {
                        label: tr!("import-check-q-namexp-a3", line = line.to_string()),
                        action: CheckAction::LineAsData {
                            line,
                            kind: CheckKind::NameXPrime,
                        },
                    },
                ],
            });
            out.push(finding(
                CheckKind::NameXPrime,
                confidence,
                decision.unwrap_or(Source::File),
                tr!("import-check-namexp", line = line.to_string()),
                vec![],
                tr!(
                    "import-check-xprime-detail",
                    count = count.to_string(),
                    first = fmt_num(first),
                    last = fmt_num(last)
                ),
                main_ev,
                question,
            ));
        } else {
            let confidence = if decision.is_some() {
                Confidence::Sure
            } else {
                Confidence::Check
            };
            let question = (confidence == Confidence::Check).then(|| Question {
                title: tr!("import-check-q-xprime-title", line = line.to_string()),
                explain: tr!("import-check-q-xprime-explain", count = count.to_string()),
                support,
                answers: vec![
                    Answer {
                        label: tr!("import-check-q-xprime-a1", line = line.to_string()),
                        action: CheckAction::Confirm(CheckKind::XPrime),
                    },
                    Answer {
                        label: tr!("import-check-q-xprime-a2", line = line.to_string()),
                        action: CheckAction::XPrimeAsData { line },
                    },
                    Answer {
                        label: tr!("import-check-q-xprime-a3"),
                        action: CheckAction::PickLine(CheckKind::XPrime),
                    },
                ],
            });
            out.push(finding(
                CheckKind::XPrime,
                confidence,
                decision.unwrap_or(Source::File),
                tr!("import-check-xprime", line = line.to_string()),
                vec![],
                tr!(
                    "import-check-xprime-detail",
                    count = count.to_string(),
                    first = fmt_num(first),
                    last = fmt_num(last)
                ),
                main_ev,
                question,
            ));
        }
    }

    // Points announced by the file.
    if let Some((a, b)) = data {
        if let Some(entry) = keys.iter().find(|k| norm_key(&k.key) == "NPOINTS") {
            if let Ok(declared) = entry.value.trim().parse::<i64>() {
                let decoded = sniff_result
                    .checks
                    .iter()
                    .map(|c| match c {
                        stokesforge_import::sniff::Check::NPoints { read, .. } => *read,
                    })
                    .next();
                let read = match (jcamp, decoded) {
                    (true, Some(read)) => read,
                    _ => b - a + 1,
                };
                let cell = value_cell(map, recipe, entry.line - 1);
                let decision = ctx.decision(CheckKind::Points);
                let settled = decision.is_some();
                let ok = declared == read as i64;
                let confidence = if ok || decision.is_some() {
                    Confidence::Sure
                } else {
                    Confidence::Check
                };
                found(
                    &mut rows,
                    CheckKind::Points,
                    FoundGroup::Data,
                    tr!("import-found-label-points"),
                    read.to_string(),
                    FoundRole::None,
                    if ok {
                        tr!(
                            "import-found-source-points-ok",
                            key = entry.key.clone(),
                            line = entry.line.to_string()
                        )
                    } else {
                        tr!(
                            "import-found-source-points-bad",
                            key = entry.key.clone(),
                            line = entry.line.to_string(),
                            declared = declared.to_string()
                        )
                    },
                    confidence,
                    vec![
                        Evidence {
                            line: entry.line,
                            cell: Some(cell),
                        },
                        Evidence {
                            line: b + 1,
                            cell: None,
                        },
                    ],
                    None,
                );
                let question = (confidence == Confidence::Check).then(|| Question {
                    title: tr!(
                        "import-check-q-points-title",
                        declared = declared.to_string(),
                        read = read.to_string()
                    ),
                    explain: tr!(
                        "import-check-q-points-explain",
                        declared = declared.to_string(),
                        read = read.to_string()
                    ),
                    support: vec![(
                        Evidence {
                            line: b + 1,
                            cell: None,
                        },
                        tr!("import-check-support-last", line = (b + 1).to_string()),
                    )],
                    answers: vec![
                        Answer {
                            label: tr!("import-check-q-points-a1", read = read.to_string()),
                            action: CheckAction::Confirm(CheckKind::Points),
                        },
                        Answer {
                            label: tr!("import-check-q-points-a2"),
                            action: CheckAction::PickLine(CheckKind::Points),
                        },
                    ],
                });
                out.push(finding(
                    CheckKind::Points,
                    confidence,
                    decision.unwrap_or(Source::File),
                    if ok {
                        tr!(
                            "import-check-npoints",
                            count = read.to_string(),
                            line = entry.line.to_string()
                        )
                    } else if settled {
                        tr!(
                            "import-check-npoints-kept",
                            read = read.to_string(),
                            declared = declared.to_string()
                        )
                    } else {
                        tr!(
                            "import-check-npoints-bad",
                            declared = declared.to_string(),
                            read = read.to_string()
                        )
                    },
                    vec![],
                    String::new(),
                    vec![
                        Evidence {
                            line: entry.line,
                            cell: Some(cell),
                        },
                        Evidence {
                            line: b + 1,
                            cell: None,
                        },
                    ],
                    question,
                ));
            }
        }
    }

    // Name of the curves.
    {
        let name = prop_origin(&ctx, &keys, PropField::LongName, true);
        if let (Some(origin), None) = (&name, ln_line) {
            let many = ctx.n_y > 3;
            let decision = ctx.decision(CheckKind::YName);
            let confidence = if many && decision.is_none() {
                Confidence::Check
            } else {
                Confidence::Sure
            };
            let key = origin.key.clone().unwrap_or_default();
            let ev = vec![Evidence {
                line: origin.line,
                cell: Some(origin.cell),
            }];
            let question = (confidence == Confidence::Check).then(|| Question {
                title: tr!("import-check-q-yname-title"),
                explain: tr!(
                    "import-check-q-yname-explain",
                    key = key.clone(),
                    count = ctx.n_y.to_string(),
                    name = origin.text.clone()
                ),
                support: Vec::new(),
                answers: vec![
                    Answer {
                        label: tr!(
                            "import-check-q-yname-a1",
                            count = ctx.n_y.to_string(),
                            name = origin.text.clone()
                        ),
                        action: CheckAction::Confirm(CheckKind::YName),
                    },
                    Answer {
                        label: tr!("import-check-q-yname-a2"),
                        action: CheckAction::NameForSheetOnly { key: key.clone() },
                    },
                    Answer {
                        label: tr!("import-check-q-yname-a3"),
                        action: CheckAction::NoName { key: key.clone() },
                    },
                ],
            });
            found(
                &mut rows,
                CheckKind::YName,
                FoundGroup::Columns,
                tr!("import-found-label-longname"),
                origin.text.clone(),
                FoundRole::LongName,
                if many {
                    tr!(
                        "import-found-source-key-all",
                        key = key.clone(),
                        line = origin.line.to_string(),
                        count = ctx.n_y.to_string()
                    )
                } else {
                    tr!(
                        "import-found-source-key",
                        key = key.clone(),
                        line = origin.line.to_string()
                    )
                },
                confidence,
                ev.clone(),
                Some(ChipId::Key(CheckKind::YName)),
            );
            let sentence = if many {
                tr!(
                    "import-check-yname-many",
                    count = ctx.n_y.to_string(),
                    c0 = marker(0)
                )
            } else {
                tr!(
                    "import-check-yname",
                    c0 = marker(0),
                    key = key.clone(),
                    line = origin.line.to_string()
                )
            };
            out.push(finding(
                CheckKind::YName,
                confidence,
                decision.unwrap_or(Source::File),
                sentence,
                vec![(origin.text.clone(), ChipId::Key(CheckKind::YName))],
                if many {
                    tr!(
                        "import-check-yname-detail",
                        key = key,
                        line = origin.line.to_string()
                    )
                } else {
                    String::new()
                },
                ev,
                question,
            ));
            if let Some(last) = out.last_mut() {
                last.key = origin.key.clone();
            }
        } else if let (Some(i), None) = (ln_line, xp_line.filter(|x| Some(*x) == ln_line)) {
            found(
                &mut rows,
                CheckKind::YName,
                FoundGroup::Columns,
                tr!("import-found-label-longname"),
                tr!("import-found-value-line", line = (i + 1).to_string()),
                FoundRole::LongName,
                tr!("import-found-source-line-names"),
                Confidence::Sure,
                vec![Evidence {
                    line: i + 1,
                    cell: Some(0),
                }],
                None,
            );
            out.push(finding(
                CheckKind::YName,
                Confidence::Sure,
                source_or_file(&ctx, CheckKind::YName),
                tr!("import-check-yname-line", line = (i + 1).to_string()),
                vec![],
                String::new(),
                vec![Evidence {
                    line: i + 1,
                    cell: Some(0),
                }],
                None,
            ));
        }
    }

    // Sheet name.
    {
        let (sentence, chips, ev) = match &recipe.name_source {
            NameSource::HeaderKey(k) if !recipe.sheet_name_from_file => {
                let wanted = norm_key(k);
                match keys.iter().find(|e| norm_key(&e.key) == wanted) {
                    Some(e) => (
                        tr!(
                            "import-check-sheetname-key",
                            c0 = marker(0),
                            key = e.key.clone()
                        ),
                        vec![(
                            e.value.trim().to_string(),
                            ChipId::Key(CheckKind::SheetName),
                        )],
                        vec![Evidence {
                            line: e.line,
                            cell: Some(value_cell(map, recipe, e.line - 1)),
                        }],
                    ),
                    None => (tr!("import-check-sheetname-file"), vec![], vec![]),
                }
            }
            // The file name, by the recipe or because the file has no title
            // (F2 of #488: the setting of the Advanced fold).
            _ => (tr!("import-check-sheetname-file"), vec![], vec![]),
        };
        {
            let (value, src) = match (&recipe.name_source, chips.first()) {
                (NameSource::HeaderKey(k), Some((text, _))) => (
                    text.clone(),
                    tr!(
                        "import-found-source-key",
                        key = k.clone(),
                        line = ev.first().map_or(0, |e| e.line).to_string()
                    ),
                ),
                // Always a second line, so the Advanced setting changes no
                // height above the control just clicked.
                _ if recipe.sheet_name_from_file => (
                    tr!("import-found-value-filename"),
                    tr!("import-found-source-filename-setting"),
                ),
                _ => (
                    tr!("import-found-value-filename"),
                    tr!("import-found-source-filename-no-title"),
                ),
            };
            found(
                &mut rows,
                CheckKind::SheetName,
                FoundGroup::Sheet,
                tr!("import-found-label-sheet"),
                value,
                FoundRole::None,
                src,
                Confidence::Sure,
                ev.clone(),
                chips.first().map(|_| ChipId::Key(CheckKind::SheetName)),
            );
        }
        out.push(finding(
            CheckKind::SheetName,
            Confidence::Sure,
            source_or_file(&ctx, CheckKind::SheetName),
            sentence,
            chips,
            String::new(),
            ev,
            None,
        ));
        if let (NameSource::HeaderKey(k), false, Some(last)) = (
            &recipe.name_source,
            recipe.sheet_name_from_file,
            out.last_mut(),
        ) {
            last.key = Some(k.clone());
        }
    }

    // Units: the header key (XUNITS, YUNITS) first, the footer axis only when
    // the key is missing; both present and different is a doubt.
    for (kind, y) in [(CheckKind::XUnit, false), (CheckKind::YUnit, true)] {
        let origin =
            prop_origin(&ctx, &keys, PropField::Units, y).or_else(|| units_line_origin(&ctx, y));
        match origin {
            Some(o) => {
                let own = o.key.as_deref().map(norm_key);
                let candidates: [&str; 2] = if y {
                    ["YUNITS", "VERTICAL AXIS"]
                } else {
                    ["XUNITS", "HORIZONTAL AXIS"]
                };
                let other = own.as_ref().and_then(|own| {
                    keys.iter()
                        .filter(|k| candidates.contains(&norm_key(&k.key).as_str()))
                        .find(|k| &norm_key(&k.key) != own)
                        .and_then(|k| {
                            let (whole, bracket) = split_name_unit(&k.value);
                            let text =
                                stokesforge_import::props::short_unit(&bracket.unwrap_or(whole));
                            (!text.is_empty()).then_some((k.clone(), text))
                        })
                });
                let decision = ctx.decision(kind);
                let disagree = other
                    .as_ref()
                    .is_some_and(|(_, text)| !text.eq_ignore_ascii_case(&o.text));
                let confidence = if disagree && decision.is_none() {
                    Confidence::Check
                } else {
                    Confidence::Sure
                };
                let axis = if y { "Y" } else { "X" };
                let own_key = o.key.clone().unwrap_or_default();
                let mut ev = vec![Evidence {
                    line: o.line,
                    cell: Some(o.cell),
                }];
                if let Some((entry, _)) = &other {
                    ev.push(Evidence {
                        line: entry.line,
                        cell: Some(value_cell(map, recipe, entry.line - 1)),
                    });
                }
                let source = match (&other, &o.key) {
                    (Some((entry, text)), Some(_)) if disagree => tr!(
                        "import-found-source-key-differs",
                        key = own_key.clone(),
                        line = o.line.to_string(),
                        other = entry.key.clone(),
                        text = text.clone()
                    ),
                    (Some((entry, _)), Some(_)) => tr!(
                        "import-found-source-key-agrees",
                        key = own_key.clone(),
                        line = o.line.to_string(),
                        other = entry.key.clone()
                    ),
                    (None, Some(_)) => tr!(
                        "import-found-source-key",
                        key = own_key.clone(),
                        line = o.line.to_string()
                    ),
                    (_, None) => tr!("import-found-source-line", line = o.line.to_string()),
                };
                found(
                    &mut rows,
                    kind,
                    FoundGroup::Columns,
                    if y {
                        tr!("import-found-label-yunits")
                    } else {
                        tr!("import-found-label-xunits")
                    },
                    o.text.clone(),
                    FoundRole::Units,
                    source,
                    confidence,
                    ev.clone(),
                    Some(ChipId::Key(kind)),
                );
                let question = (confidence == Confidence::Check)
                    .then(|| {
                        let (entry, text) = other.clone()?;
                        Some(Question {
                            title: tr!("import-check-q-units-title", axis = axis),
                            explain: tr!(
                                "import-check-q-units-explain",
                                key = own_key.clone(),
                                text = o.text.clone(),
                                other = entry.key.clone(),
                                other_text = text.clone()
                            ),
                            support: vec![
                                (
                                    ev[0],
                                    tr!(
                                        "import-check-support-key",
                                        key = own_key.clone(),
                                        value = o.text.clone()
                                    ),
                                ),
                                (
                                    ev[1],
                                    tr!(
                                        "import-check-support-key",
                                        key = entry.key.clone(),
                                        value = text.clone()
                                    ),
                                ),
                            ],
                            answers: vec![
                                Answer {
                                    label: tr!(
                                        "import-check-q-units-a1",
                                        text = o.text.clone(),
                                        key = own_key.clone()
                                    ),
                                    action: CheckAction::Confirm(kind),
                                },
                                Answer {
                                    label: tr!(
                                        "import-check-q-units-a2",
                                        text = text.clone(),
                                        key = entry.key.clone()
                                    ),
                                    action: CheckAction::UnitFromKey {
                                        kind,
                                        key: entry.key.clone(),
                                    },
                                },
                            ],
                        })
                    })
                    .flatten();
                let sentence = match (&o.key, y) {
                    (Some(_), false) => tr!(
                        "import-check-xunit",
                        c0 = marker(0),
                        key = o.key.clone().unwrap_or_default(),
                        line = o.line.to_string()
                    ),
                    (Some(_), true) => tr!(
                        "import-check-yunit",
                        c0 = marker(0),
                        key = o.key.clone().unwrap_or_default(),
                        line = o.line.to_string()
                    ),
                    (None, false) => tr!(
                        "import-check-xunit-line",
                        c0 = marker(0),
                        line = o.line.to_string()
                    ),
                    (None, true) => tr!(
                        "import-check-yunit-line",
                        c0 = marker(0),
                        line = o.line.to_string()
                    ),
                };
                out.push(finding(
                    kind,
                    confidence,
                    source_or_file(&ctx, kind),
                    sentence,
                    vec![(o.text.clone(), ChipId::Key(kind))],
                    String::new(),
                    ev,
                    question,
                ));
                if let Some(last) = out.last_mut() {
                    last.key = o.key.clone();
                }
            }
            None => {
                // The recipe (a template without unit roles) reads no unit,
                // but the header may give one: say so rather than "not in
                // the file".
                let candidates: [&str; 2] = if y {
                    ["YUNITS", "VERTICAL AXIS"]
                } else {
                    ["XUNITS", "HORIZONTAL AXIS"]
                };
                let unused = keys
                    .iter()
                    .filter(|k| candidates.contains(&norm_key(&k.key).as_str()))
                    .find_map(|k| {
                        let (whole, bracket) = split_name_unit(&k.value);
                        let text = stokesforge_import::props::short_unit(&bracket.unwrap_or(whole));
                        (!text.is_empty()).then(|| (k.clone(), text))
                    });
                let axis = if y { "Y" } else { "X" };
                match unused {
                    Some((entry, text)) => out.push(finding(
                        kind,
                        Confidence::Missing,
                        Source::File,
                        tr!(
                            "import-check-unit-unused",
                            axis = axis,
                            key = entry.key.clone(),
                            line = entry.line.to_string(),
                            text = text
                        ),
                        vec![],
                        String::new(),
                        vec![Evidence {
                            line: entry.line,
                            cell: Some(value_cell(map, recipe, entry.line - 1)),
                        }],
                        None,
                    )),
                    None => out.push(finding(
                        kind,
                        Confidence::Missing,
                        Source::File,
                        if y {
                            tr!("import-check-missing-yunit")
                        } else {
                            tr!("import-check-missing-xunit")
                        },
                        vec![],
                        String::new(),
                        vec![],
                        None,
                    )),
                }
            }
        }
    }
    if xp_line.is_some() {
        out.push(finding(
            CheckKind::XPrimeUnit,
            Confidence::Missing,
            Source::File,
            tr!("import-check-missing-xpunit"),
            vec![],
            String::new(),
            vec![],
            None,
        ));
    }

    // Comments read from a key.
    if let Some(o) = prop_origin(&ctx, &keys, PropField::Comment, true) {
        found(
            &mut rows,
            CheckKind::Comment,
            FoundGroup::Columns,
            tr!("import-found-label-comments"),
            o.text.clone(),
            FoundRole::Comments,
            tr!(
                "import-found-source-key",
                key = o.key.clone().unwrap_or_default(),
                line = o.line.to_string()
            ),
            Confidence::Sure,
            vec![Evidence {
                line: o.line,
                cell: Some(o.cell),
            }],
            Some(ChipId::Key(CheckKind::Comment)),
        );
    }

    // Footer.
    if let Some((_, b)) = data {
        let rest: Vec<usize> = (b + 1..map.line_count())
            .filter(|&i| !map.line(i).trim().is_empty())
            .collect();
        if let Some(&first) = rest.first() {
            let kept = recipe.keep_footer_metadata;
            found(
                &mut rows,
                CheckKind::Footer,
                FoundGroup::Data,
                tr!("import-found-label-footer"),
                tr!("import-found-value-n-lines", count = rest.len().to_string()),
                FoundRole::None,
                if kept {
                    tr!("import-found-source-footer-kept")
                } else {
                    tr!("import-found-source-footer-dropped")
                },
                Confidence::Sure,
                vec![Evidence {
                    line: first + 1,
                    cell: None,
                }],
                None,
            );
            out.push(finding(
                CheckKind::Footer,
                Confidence::Sure,
                source_or_file(&ctx, CheckKind::Footer),
                if kept {
                    tr!("import-check-footer-kept", count = rest.len().to_string())
                } else {
                    tr!(
                        "import-check-footer-dropped",
                        count = rest.len().to_string()
                    )
                },
                vec![],
                String::new(),
                vec![Evidence {
                    line: first + 1,
                    cell: None,
                }],
                None,
            ));
        }
    }

    // The doubts first, then what is missing, then what is sure.
    let rank = |c: Confidence| match c {
        Confidence::Check => 0,
        Confidence::Missing => 1,
        Confidence::Sure => 2,
    };
    out.sort_by_key(|f| rank(f.confidence));
    let n_check = out
        .iter()
        .filter(|f| f.confidence == Confidence::Check)
        .count();
    let n_missing = out
        .iter()
        .filter(|f| f.confidence == Confidence::Missing)
        .count();
    let n_sure = out.len() - n_check - n_missing;
    CheckView {
        fingerprint,
        findings: out,
        n_check,
        n_missing,
        n_sure,
        template,
        receipt,
        keys,
        note,
        data: data.map(|(a, b)| (a + 1, b + 1)),
        rows: sort_rows(rows),
        n_rows,
        n_cols,
        decimal_sample,
        encoding_detail,
        encoding_name,
        detected: DetectedFormat {
            separator: sniff_result.recipe.separator.clone(),
            decimal: sniff_result.recipe.decimal,
            thousands: sniff_result.recipe.thousands,
            encoding: sniff_result.recipe.encoding,
        },
    }
}

/// Display order of « What was found »: Data, Columns, Sheet, each group in
/// the order the rows were found.
fn sort_rows(mut rows: Vec<FoundRow>) -> Vec<FoundRow> {
    let rank = |g: FoundGroup| match g {
        FoundGroup::Data => 0,
        FoundGroup::Columns => 1,
        FoundGroup::Sheet => 2,
    };
    rows.sort_by_key(|r| rank(r.group));
    rows
}

/// The sentence « You get: … » from a parsed table.
pub fn receipt_text(parsed: &ParsedTable, sheets: usize) -> String {
    let cols = parsed.columns.len();
    let rows = parsed.row_count();
    let mut text = tr!(
        "import-receipt",
        sheets = sheets,
        columns = cols.to_string(),
        rows = rows.to_string()
    );
    let unit = |index: usize| {
        parsed
            .columns
            .get(index)
            .and_then(|c| c.unit.clone())
            .filter(|u| !u.is_empty())
    };
    if let Some(u) = unit(0) {
        text.push_str(&format!(" · X: {u}"));
    }
    if let Some(u) = unit(1) {
        text.push_str(&format!(" · Y: {u}"));
    }
    let xp: Vec<f64> = parsed.columns.iter().filter_map(|c| c.x_prime).collect();
    if let (Some(lo), Some(hi)) = (
        xp.iter().copied().reduce(f64::min),
        xp.iter().copied().reduce(f64::max),
    ) {
        text.push_str(&format!(" · x′: {} – {}", fmt_num(lo), fmt_num(hi)));
    }
    text
}

/// Test seam and CLI-like entry: the view of a file read from its bytes with
/// the detector's own recipe, nothing decided.
pub fn analyse(bytes: &[u8], name: &str) -> Result<CheckView, SniffRefusal> {
    let found = sniff(bytes, name)?;
    let map = file_map(bytes, &found.recipe).map_err(|_| SniffRefusal::NoNumbers)?;
    let facts = FileFacts::new(Path::new(name), bytes, Some(&found));
    let fp = Arc::new(compute_fingerprint(&map, &found, facts));
    Ok(build_view(
        &map,
        &found,
        &found.recipe,
        None,
        fp,
        None,
        None,
        String::new(),
    ))
}

// -- The state -------------------------------------------------------------

impl AppState {
    /// The file shown and the kind it belongs to (the key of its remembered
    /// answers), when the detector read it.
    pub(super) fn check_kind_id(&self) -> Option<(PathBuf, String)> {
        let path = self.import_preview_path()?;
        let state = self.import_state.as_ref()?;
        let found = state.auto.sniffs.get(&path)?.as_ref().ok()?;
        Some((path, stokesforge_import::kind_id(found)))
    }

    /// The fingerprint of the file shown (computed once per file).
    pub fn import_fingerprint(&self) -> Option<Arc<Fingerprint>> {
        let path = self.import_preview_path()?;
        let state = self.import_state.as_ref()?;
        if let Some(found) = state.checks.cached_fingerprint(&path) {
            return Some(found);
        }
        let found = state.auto.sniffs.get(&path)?.as_ref().ok()?;
        let bytes = self.import_bytes(&path).ok()?;
        state.checks.fingerprint(&path, &bytes, found)
    }

    /// What the user already settled for the kind of `path`: a template's
    /// remembered answers are "from the template", the others "from you".
    fn check_decisions(&self, path: &Path, found: &Sniff) -> HashMap<CheckKind, Source> {
        let Some(state) = self.import_state.as_ref() else {
            return HashMap::new();
        };
        let kind = stokesforge_import::kind_id(found);
        let source = state
            .checks
            .template_of_file
            .get(path)
            .map_or(Source::You, |name| Source::Template(name.clone()));
        CheckKind::ALL
            .into_iter()
            .filter(|k| state.store.is_remembered(&kind, k.doubt_id()))
            .map(|k| (k, source.clone()))
            .collect()
    }

    /// The check view of the file shown, cached while neither the file, the
    /// recipe, the answers nor the templates change.
    pub fn import_check(&self) -> Option<Arc<CheckView>> {
        let state = self.import_state.as_ref()?;
        let path = self.import_preview_path()?;
        let recipe = state.preview_recipe();
        {
            let cache = state.checks.cache.borrow();
            if let Some((key, view)) = cache.as_ref() {
                if key.0 == path && key.2 == state.checks.generation && &key.1 == recipe {
                    return Some(view.clone());
                }
            }
        }
        let found = state.auto.sniffs.get(&path)?.as_ref().ok()?;
        let map = self.import_file_map()?;
        let map = map.as_ref().as_ref().ok()?;
        let fingerprint = self.import_fingerprint()?;
        let decided = self.check_decisions(&path, found);
        let template = state.checks.template_of_file.get(&path).cloned();
        let encoding = self.import_encoding();
        let receipt = match &*self.import_parsed(&path) {
            Ok(parsed) => receipt_text(
                parsed,
                state
                    .paths
                    .iter()
                    .filter(|p| matches!(state.auto.sniffs.get(*p), Some(Ok(_))))
                    .count()
                    .max(1),
            ),
            Err(_) => tr!("import-grid-strip-error"),
        };
        let view = Arc::new(build_view(
            map,
            found,
            recipe,
            Some(&decided),
            fingerprint,
            template,
            encoding,
            receipt,
        ));
        *state.checks.cache.borrow_mut() = Some((
            (path, recipe.clone(), state.checks.generation),
            view.clone(),
        ));
        Some(view)
    }

    /// Remembers, for the kind of file shown, that the user answered the
    /// doubt `id` (checked it, marked it as checked or corrected it). The
    /// store is written at once; a write failure goes to the status bar.
    pub(super) fn check_remember(&mut self, id: &str) {
        let Some((_, kind)) = self.check_kind_id() else {
            return;
        };
        self.remember_answer_of(&kind, id);
    }

    /// [`AppState::check_remember`] for a given kind of file.
    pub(super) fn remember_answer_of(&mut self, kind: &str, id: &str) {
        let Some(state) = self.import_state.as_mut() else {
            return;
        };
        state.store.remember_answer(kind, id);
        state.checks.generation += 1;
        self.save_import_store();
    }

    /// Writes the template store; a failure is a status-bar message.
    pub(super) fn save_import_store(&mut self) {
        let Some(state) = self.import_state.as_ref() else {
            return;
        };
        if let Err(err) = state.store.save() {
            self.fail(tr!(
                "error-template-save",
                cause = crate::errors::io::error_message(&err)
            ));
        }
    }

    /// Settles a finding for the kind of file shown: it is not raised again
    /// for a later file of that kind.
    pub fn check_decide(&mut self, kind: CheckKind) {
        self.check_remember(kind.doubt_id());
    }

    /// A manual change of a setting settles that finding.
    pub fn check_touch(&mut self, kind: CheckKind) {
        self.check_decide(kind);
    }

    // -- Review ---------------------------------------------------------------

    pub fn check_review_start(&mut self) {
        let Some(view) = self.import_check() else {
            return;
        };
        let order: Vec<CheckKind> = view.doubts().iter().map(|f| f.kind).collect();
        if order.is_empty() {
            return;
        }
        if let Some(state) = self.import_state.as_mut() {
            state.checks.review = Some(Review {
                order,
                index: 0,
                log: Vec::new(),
            });
            state.checks.review_done = None;
            state.checks.pick = None;
        }
    }

    pub fn check_review_start_at(&mut self, kind: CheckKind) {
        self.check_review_start();
        if let Some(review) = self
            .import_state
            .as_mut()
            .and_then(|s| s.checks.review.as_mut())
        {
            if let Some(i) = review.order.iter().position(|k| *k == kind) {
                review.index = i;
            }
        }
    }

    pub fn check_review_stop(&mut self) {
        if let Some(state) = self.import_state.as_mut() {
            state.checks.review = None;
            state.checks.pick = None;
        }
    }

    pub fn check_review_back(&mut self) {
        if let Some(review) = self
            .import_state
            .as_mut()
            .and_then(|s| s.checks.review.as_mut())
        {
            review.index = review.index.saturating_sub(1);
        }
        if let Some(state) = self.import_state.as_mut() {
            state.checks.pick = None;
        }
    }

    /// Moves to the next doubt, or ends the review.
    pub fn check_review_next(&mut self, log: Option<String>) {
        let path = self.import_preview_path();
        let Some(state) = self.import_state.as_mut() else {
            return;
        };
        state.checks.pick = None;
        let Some(review) = state.checks.review.as_mut() else {
            return;
        };
        if let Some(line) = log {
            review.log.push(line);
        }
        review.index += 1;
        if review.index >= review.order.len() {
            let done = review.log.clone();
            state.checks.review = None;
            if let Some(path) = path {
                state.checks.review_done = Some((path, done));
            }
        }
    }

    /// The doubt the review is on.
    pub fn check_review_info(&self) -> Option<ReviewInfo> {
        let state = self.import_state.as_ref()?;
        let review = state.checks.review.as_ref()?;
        let kind = *review.order.get(review.index)?;
        let view = self.import_check()?;
        let finding = view.finding(kind)?.clone();
        Some(ReviewInfo {
            index: review.index,
            total: review.order.len(),
            finding,
        })
    }

    /// Skips the doubts that stopped being doubts (an answer may settle or
    /// remove another one). Once per frame.
    pub fn check_review_sync(&mut self) {
        loop {
            let Some(kind) = self.check_review_kind() else {
                return;
            };
            let Some(view) = self.import_check() else {
                return;
            };
            let live = view
                .finding(kind)
                .is_some_and(|f| f.confidence == Confidence::Check && f.question.is_some());
            if live {
                return;
            }
            self.check_review_next(None);
        }
    }

    pub fn check_review_kind(&self) -> Option<CheckKind> {
        let review = self.import_state.as_ref()?.checks.review.as_ref()?;
        review.order.get(review.index).copied()
    }

    pub fn check_review_done(&self) -> Option<Vec<String>> {
        let state = self.import_state.as_ref()?;
        let (path, log) = state.checks.review_done.as_ref()?;
        (Some(path) == self.import_preview_path().as_ref()).then(|| log.clone())
    }

    pub fn check_review_done_close(&mut self) {
        if let Some(state) = self.import_state.as_mut() {
            state.checks.review_done = None;
        }
    }

    pub fn check_pick(&self) -> Option<Pick> {
        self.import_state.as_ref()?.checks.pick
    }

    pub fn check_set_pick(&mut self, pick: Option<Pick>) {
        if let Some(state) = self.import_state.as_mut() {
            state.checks.pick = pick;
        }
    }

    // -- Recipe changes asked by an answer -----------------------------------

    /// Comma groups thousands: the decimal is the point, thousands the comma.
    pub fn check_comma_groups_thousands(&mut self) {
        if let Some(state) = self.import_state.as_mut() {
            state.recipe.decimal = DecimalStyle::Dot;
            state.recipe.thousands = Some(',');
        }
        self.check_decide(CheckKind::Decimal);
    }

    /// Drops the Long Name of the Y columns read from `key`.
    pub fn check_drop_name_prop(&mut self, key: &str, keep_sheet: bool) {
        let wanted = norm_key(key);
        if let Some(state) = self.import_state.as_mut() {
            let recipe = &mut state.recipe;
            recipe
                .header_props
                .retain(|p| !(norm_key(&p.key) == wanted && p.field == PropField::LongName));
            if !keep_sheet
                && matches!(&recipe.name_source, NameSource::HeaderKey(k) if norm_key(k) == wanted)
            {
                recipe.name_source = NameSource::Filename;
            }
        }
        self.check_decide(CheckKind::YName);
    }
}

// -- Tests -------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../test/import")
    }

    fn view_of(name: &str) -> Result<CheckView, SniffRefusal> {
        let bytes = std::fs::read(dir().join(name)).unwrap();
        analyse(&bytes, name)
    }

    fn kinds(view: &CheckView, confidence: Confidence) -> Vec<CheckKind> {
        view.findings
            .iter()
            .filter(|f| f.confidence == confidence)
            .map(|f| f.kind)
            .collect()
    }

    /// Public files of `test/import/`: how many doubts each raises, and which.
    #[test]
    fn doubts_per_public_file() {
        use CheckKind::*;
        let table: [(&str, Vec<CheckKind>); 10] = [
            ("analytikjena_nohead_semicolon.csv", vec![]),
            ("generic_named_header.csv", vec![]),
            ("ir_jcamp_lookalike.txt", vec![]),
            ("irradiation_wide_matrix.txt", vec![NameXPrime]),
            ("jasco_english_locale_dot.csv", vec![]),
            ("jasco_fluo_tab.txt", vec![]),
            ("jasco_npoints_mismatch.txt", vec![Points]),
            ("JascoUV_vis.csv", vec![]),
            ("jasco_uvvis_semicolon.csv", vec![]),
            ("rpe_nohead_tab.asc", vec![]),
        ];
        for (name, expected) in table {
            let view = view_of(name).unwrap_or_else(|e| panic!("{name} refused: {e:?}"));
            assert_eq!(kinds(&view, Confidence::Check), expected, "{name}");
            assert_eq!(view.n_check, expected.len(), "{name}");
        }
    }

    /// The x' doubt (and the missing x' unit) exists above five columns only.
    #[test]
    fn x_prime_only_above_five_columns() {
        // A short file with a times-looking line raises no x' finding.
        let short = "TITLE;a\nXYDATA\n;0;15;30\n800;1;2;3\n799;1;2;3\n";
        let view = analyse(short.as_bytes(), "a.csv").unwrap();
        assert!(view.finding(CheckKind::XPrime).is_none());
        assert!(view.finding(CheckKind::XPrimeUnit).is_none());
        let wide = "TITLE;a\nXYDATA\n;0;15;30;45;60;75\n800;1;2;3;4;5;6\n799;1;2;3;4;5;6\n";
        let view = analyse(wide.as_bytes(), "a.csv").unwrap();
        assert!(view.finding(CheckKind::XPrime).is_some());
        assert!(!x_prime_applies(2));
        assert!(!x_prime_applies(5));
        assert!(x_prime_applies(6));
        assert!(x_prime_applies(241));
    }

    /// A JCAMP `(X++(Y..Y))` file read by its own reader (#488): lines of 7
    /// then 3 values give 2 columns and 10 points, so neither the points
    /// (2 lines), nor the uneven lines, nor the curve name is a doubt.
    #[test]
    fn a_jcamp_table_raises_no_line_based_doubt() {
        let text = "##TITLE=demo\n##JCAMP-DX=4.24\n##DATA TYPE=INFRARED SPECTRUM\n\
                    ##XUNITS=1/CM\n##YUNITS=ABSORBANCE\n##XFACTOR=1\n##YFACTOR=1\n\
                    ##FIRSTX=100\n##LASTX=109\n##NPOINTS=10\n##XYDATA=(X++(Y..Y))\n\
                    100 1 2 3 4 5 6 7\n107 8 9 10\n##END=\n";
        let view = analyse(text.as_bytes(), "demo.jdx").unwrap();
        assert_eq!(kinds(&view, Confidence::Check), Vec::<CheckKind>::new());
        let points = view.finding(CheckKind::Points).expect("points finding");
        assert_eq!(points.confidence, Confidence::Sure);
    }

    #[test]
    fn binary_file_is_refused() {
        assert!(matches!(
            view_of("proprietary_binary.dat"),
            Err(SniffRefusal::Binary)
        ));
    }

    /// The anonymized kinetic (skipped when absent): the times and the single
    /// name of 240 curves are doubts, the units of x' are not found.
    #[test]
    fn anonymized_kinetic_raises_two_doubts() {
        let path = std::env::var("STOKESFORGE_TEST_KINETIC").unwrap_or_else(|_| {
            "/tmp/claude-1000/-home-aliocha-dev-project-StokesForge/0057e074-3b5e-40d6-8414-b4d9c5057c54/scratchpad/proto488b/demo_kinetic.csv".to_string()
        });
        let Ok(bytes) = std::fs::read(&path) else {
            return;
        };
        let view = analyse(&bytes, "demo_kinetic.csv").unwrap();
        assert_eq!(
            kinds(&view, Confidence::Check),
            vec![CheckKind::XPrime, CheckKind::YName]
        );
        assert_eq!(
            kinds(&view, Confidence::Missing),
            vec![CheckKind::XPrimeUnit]
        );
        assert!(view.fingerprint.recognised);
        assert!(view.fingerprint.kinetic);
        assert_eq!(view.fingerprint.curves, 240);
        assert_eq!(view.fingerprint.model.as_deref(), Some("V-770"));
        // The notes repeat the number of spectra.
        let xp = view.finding(CheckKind::XPrime).unwrap();
        let support = &xp.question.as_ref().unwrap().support;
        assert!(support.len() >= 2, "{support:?}");
    }

    #[test]
    fn plain_tables_miss_their_units_and_are_not_recognised() {
        let view = view_of("analytikjena_nohead_semicolon.csv").unwrap();
        assert!(!view.fingerprint.recognised);
        assert_eq!(
            kinds(&view, Confidence::Missing),
            vec![CheckKind::XUnit, CheckKind::YUnit]
        );
    }

    /// A template says « times » but this file has data on that line: the
    /// decision is taken back and the doubt returns, with its note.
    #[test]
    fn a_template_decision_is_taken_back_when_the_line_looks_like_data() {
        let good = "TITLE;a\nXYDATA\n;0;15;30;45;60;75\n800;1;2;3;4;5;6\n799;1;2;3;4;5;6\n";
        let found = sniff(good.as_bytes(), "a.csv").unwrap();
        assert!(found.recipe.subheader_roles.x_prime.is_some());
        let bad = "TITLE;a\nXYDATA\n801;7;8;9;10;11;12\n800;1;2;3;4;5;6\n799;1;2;3;4;5;6\n";
        let map = file_map(bad.as_bytes(), &found.recipe).unwrap();
        let facts = FileFacts::new(Path::new("a.csv"), bad.as_bytes(), Some(&found));
        let fp = Arc::new(compute_fingerprint(&map, &found, facts));
        let mut decided = HashMap::new();
        decided.insert(CheckKind::XPrime, Source::Template("t".into()));
        let view = build_view(
            &map,
            &found,
            &found.recipe,
            Some(&decided),
            fp,
            Some("t".into()),
            None,
            String::new(),
        );
        assert!(view.note.is_some());
        assert_eq!(
            view.finding(CheckKind::XPrime).map(|f| f.confidence),
            Some(Confidence::Check)
        );
    }

    #[test]
    fn fingerprint_id_ignores_header_length() {
        let bytes = std::fs::read(dir().join("JascoUV_vis.csv")).unwrap();
        let text = String::from_utf8_lossy(&bytes).to_string();
        // Three more lines in the header, as another run of the same instrument.
        let longer = text.replacen("OWNER;", "NOTE;a\nNOTE;b\nNOTE;c\nOWNER;", 1);
        let a = analyse(&bytes, "a.csv").unwrap();
        let b = analyse(longer.as_bytes(), "b.csv").unwrap();
        assert_eq!(a.fingerprint.id, b.fingerprint.id);
        assert!(a.fingerprint.recognised);
    }

    /// The header key (XUNITS) gives the units; the footer only agrees or
    /// contradicts it, and a contradiction is a doubt with two answers.
    #[test]
    fn the_units_key_comes_first_and_a_disagreeing_footer_is_a_doubt() {
        let make = |footer_axis: &str| {
            format!(
                "TITLE;a\nXUNITS;NANOMETERS\nYUNITS;ABSORBANCE\nXYDATA\n800;1\n799;2\n798;3\n797;4\n\n\
                 [Detailed Information]\nHorizontal axis;Wavelength [{footer_axis}]\nVertical axis;Abs\n"
            )
        };
        let agree = analyse(make("nm").as_bytes(), "a.csv").unwrap();
        let x = agree
            .rows
            .iter()
            .find(|r| r.kind == CheckKind::XUnit)
            .unwrap();
        assert_eq!(x.value, "nm");
        assert_eq!(x.confidence, Confidence::Sure);
        assert!(x.source.contains("XUNITS"), "{}", x.source);
        let differ = analyse(make("cm").as_bytes(), "b.csv").unwrap();
        let f = differ.finding(CheckKind::XUnit).unwrap();
        assert_eq!(f.confidence, Confidence::Check);
        let answers = &f.question.as_ref().unwrap().answers;
        assert_eq!(answers.len(), 2);
        assert!(matches!(
            answers[1].action,
            CheckAction::UnitFromKey {
                kind: CheckKind::XUnit,
                ..
            }
        ));
    }

    /// « What was found »: Data first, then Columns, then Sheet, each row with
    /// its source; the Long Name is a role value.
    #[test]
    fn what_was_found_is_grouped_and_names_its_source() {
        let view = view_of("JascoUV_vis.csv").unwrap();
        let groups: Vec<FoundGroup> = view.rows.iter().map(|r| r.group).collect();
        let mut sorted = groups.clone();
        sorted.sort_by_key(|g| match g {
            FoundGroup::Data => 0,
            FoundGroup::Columns => 1,
            FoundGroup::Sheet => 2,
        });
        assert_eq!(groups, sorted);
        assert!(view.rows.iter().any(|r| r.group == FoundGroup::Data));
        assert!(view.rows.iter().any(|r| r.group == FoundGroup::Sheet));
        assert!(view.n_rows > 0 && view.n_cols >= 2);
        for row in &view.rows {
            assert!(!row.label.is_empty(), "{row:?}");
        }
        let name = view.rows.iter().find(|r| r.kind == CheckKind::YName);
        if let Some(name) = name {
            assert_eq!(name.role, FoundRole::LongName);
        }
    }
}

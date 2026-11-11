//! The detector. Reads the bytes of a file and proposes the
//! whole import configuration a chemist would set by hand: encoding,
//! separator, decimal, where the data start and stop, the x' line, column
//! names, the units and titles written as `key;value` lines.
//!
//! Pure, and without sentences: it returns data ([`Why`]); the application
//! turns it into text through the Fluent catalog.

use std::collections::BTreeSet;

use crate::instrument::{Instrument, InstrumentMatch};
use crate::numbers::NumberFormat;
use crate::props::{prop_text, split_kv, split_name_unit};
pub use crate::thousands::ThousandsGuess;

use crate::recipe::{
    BlockSplit, DataEnd, DecimalStyle, Encoding, HeaderLayout, HeaderProp, HeaderSkip,
    ImportRecipe, NameSource, PropField, PropTarget, Separator, SubheaderLineRange,
};

/// Why a file is not proposed an import at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SniffRefusal {
    /// Not text (the format of the device software).
    Binary,
    /// Nothing in it.
    Empty,
    /// Text, but no line of numbers.
    NoNumbers,
    /// JCAMP-DX NMR data (spectrum, FID, 2D, peak assignments): out of scope.
    Nmr,
}

/// Instrument family recognized from a signature in the file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Family {
    Jasco,
    /// `##KEY=VALUE` header lines.
    Jcamp,
    /// Numbers, with no header.
    PlainTable,
}

/// Column separator as detected.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SepKind {
    Semicolon,
    Tab,
    Comma,
    Whitespace,
    /// A single column: no separator.
    None,
}

/// How the data end was found.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EndKind {
    BlankLine,
    Marker,
    Text,
}

/// One automatic choice, as the recipe holds it.
#[derive(Debug, Clone, PartialEq)]
pub enum AutoWhat {
    Encoding,
    Separator,
    Decimal,
    DataStart,
    Marker,
    XPrime,
    Names,
    LongName,
    Units,
    SameLine,
    DataEnd,
    KeepFooter,
    NameSource,
    Prop(HeaderProp),
}

/// The reason of an automatic choice, as data (no sentence).
#[derive(Debug, Clone, PartialEq)]
pub enum Why {
    Encoding(Encoding),
    Separator {
        kind: SepKind,
        columns: usize,
    },
    Decimal {
        comma: bool,
    },
    DataStart {
        line: usize,
        rows: usize,
        columns: usize,
    },
    Marker {
        line: usize,
        text: String,
    },
    XPrime {
        line: usize,
        count: usize,
    },
    LongNameXPrime {
        line: usize,
    },
    Names {
        line: usize,
    },
    LongName {
        line: usize,
    },
    Units {
        line: usize,
    },
    SameLine {
        line: usize,
    },
    DataEnd {
        kind: EndKind,
        line: usize,
    },
    KeepFooter {
        count: usize,
    },
    NameFromKey {
        key: String,
        line: usize,
    },
    Prop {
        line: usize,
        key: String,
        value: String,
    },
}

/// An automatic choice, the file lines it rests on, and its reason.
#[derive(Debug, Clone, PartialEq)]
pub struct AutoChoice {
    pub what: AutoWhat,
    /// 1-based file lines.
    pub lines: Vec<usize>,
    pub why: Why,
}

/// A check made on the file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Check {
    /// The header announces `declared` points; `read` rows were found.
    NPoints { declared: i64, read: usize },
}

impl Check {
    pub fn ok(&self) -> bool {
        match self {
            Check::NPoints { declared, read } => *declared == *read as i64,
        }
    }
}

/// What makes two files "of the same kind": same recipe fits both.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct KindKey {
    pub family: Option<Family>,
    pub sep: SepKind,
    pub comma: bool,
    pub columns: usize,
    /// Which line roles exist (bits: x', names, long name, units, same line).
    pub roles: u8,
    /// Without a marker, the header length counts too.
    pub header_len: Option<usize>,
    pub end: Option<EndKind>,
}

/// Everything the detector proposes for a file.
#[derive(Debug, Clone, PartialEq)]
pub struct Sniff {
    pub recipe: ImportRecipe,
    pub choices: Vec<AutoChoice>,
    pub family: Option<Family>,
    /// Text of the line that gave the family (for a template).
    pub signature: String,
    pub kind: KindKey,
    pub checks: Vec<Check>,
    pub rows: usize,
    pub columns: usize,
    /// Sentinels ("Overflow", "NaN", "---"...) found in the data run; the
    /// recipe reads them as missing values.
    pub missing: Vec<MissingToken>,
    /// The separator and decimal the file stated and the detector believed.
    pub stated: Option<StatedFormat>,
    /// Every run of data rows with the width of the table, in file order
    /// (one entry for an ordinary file).
    pub blocks: Vec<DataBlock>,
    /// What the detector is not sure of, as data (no sentence).
    pub doubts: Vec<Doubt>,
    /// Set when the file is JCAMP-DX and the recipe reads it with
    /// [`crate::recipe::FileFormat::Jcamp`].
    pub jcamp: Option<crate::jcamp::JcampSummary>,
    /// The instrument or software recognised from the file's own
    /// lines (never from the extension), with the line that proves it.
    pub instrument: Option<InstrumentMatch>,
    /// The thousands separator the numbers are grouped with, when
    /// the evidence is strict ([`crate::thousands`]); `recipe.thousands` holds it.
    pub thousands: Option<ThousandsGuess>,
}

/// A run of data rows with the width of the table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DataBlock {
    /// 1-based file lines, both included.
    pub first_line: usize,
    pub last_line: usize,
    pub rows: usize,
    /// The nearest non-empty line above the block that holds no number,
    /// trimmed and cut to 80 characters.
    pub title: Option<String>,
}

/// Something the detector did or saw and the user may want to check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Doubt {
    /// One table interrupted by text lines (segment titles), read as one
    /// table: the recipe skips them. `lines` are 1-based, at most 50.
    TextRowsInData { count: usize, lines: Vec<usize> },
    /// Several runs of data rows in the file. `split`: the recipe reads each as
    /// its own table; otherwise it keeps the longest and the others are lost
    /// unless the user splits.
    SeveralBlocks {
        count: usize,
        rows_total: usize,
        split: bool,
    },
    /// Comma separator with dot decimal, but most rows look like decimal-comma
    /// numbers separated by `", "`. First such data line, 1-based.
    DecimalOrSeparator { line: usize },
    /// JCAMP-DX: a problem in block `block` (0-based, in file order): a table
    /// not read (NTUPLES page, unknown form), lines skipped, a cap hit, or a
    /// point count that differs from `NPOINTS` (blocks after the first; the
    /// first one is a [`Check`]).
    Jcamp {
        block: usize,
        issue: crate::jcamp::JcampIssue,
    },
}

impl Sniff {
    pub fn choice(&self, what: &AutoWhat) -> Option<&AutoChoice> {
        self.choices.iter().find(|c| &c.what == what)
    }
}

fn header_total(recipe: &ImportRecipe) -> usize {
    recipe.header_layout.main_header_lines
        + recipe.header_layout.subheader_lines
        + usize::from(recipe.has_column_header_row)
}

impl AutoWhat {
    /// Does `recipe` still hold the value the detector (`auto`) proposed?
    pub fn is_set(&self, recipe: &ImportRecipe, auto: &ImportRecipe) -> bool {
        let (r, a) = (recipe, auto);
        match self {
            AutoWhat::Encoding => r.encoding == a.encoding,
            AutoWhat::Separator => r.separator == a.separator,
            AutoWhat::Decimal => r.decimal == a.decimal,
            AutoWhat::DataStart => header_total(r) == header_total(a),
            AutoWhat::Marker => r.main_until_marker == a.main_until_marker,
            AutoWhat::XPrime => r.subheader_roles.x_prime == a.subheader_roles.x_prime,
            AutoWhat::Names => r.has_column_header_row == a.has_column_header_row,
            AutoWhat::LongName => r.subheader_roles.long_name == a.subheader_roles.long_name,
            AutoWhat::Units => r.subheader_roles.unit == a.subheader_roles.unit,
            AutoWhat::SameLine => {
                r.subheader_roles.same_line_name_unit == a.subheader_roles.same_line_name_unit
            }
            AutoWhat::DataEnd => r.data_end == a.data_end,
            AutoWhat::KeepFooter => r.keep_footer_metadata == a.keep_footer_metadata,
            AutoWhat::NameSource => r.name_source == a.name_source,
            AutoWhat::Prop(prop) => r.header_props.contains(prop),
        }
    }

    /// Takes the choice back: the setting returns to the plain default, and a
    /// line that lost its role goes back to the header.
    pub fn undo(&self, recipe: &mut ImportRecipe) {
        let roles = &mut recipe.subheader_roles;
        match self {
            AutoWhat::Encoding => recipe.encoding = Encoding::Auto,
            AutoWhat::Separator => recipe.separator = Separator::Auto,
            AutoWhat::Decimal => recipe.decimal = DecimalStyle::Dot,
            AutoWhat::DataStart => {
                recipe.header_layout = HeaderLayout::default();
                recipe.header = HeaderSkip::None;
                recipe.main_until_marker = None;
                recipe.has_column_header_row = false;
                roles.long_name = None;
                roles.unit = None;
                roles.comment = None;
                roles.x_prime = None;
                roles.same_line_name_unit = false;
                recipe.name_unit_slash = false;
                recipe.strip_unit_brackets = false;
            }
            AutoWhat::Marker => recipe.main_until_marker = None,
            AutoWhat::XPrime => roles.x_prime = None,
            AutoWhat::Names => {
                if recipe.has_column_header_row {
                    recipe.has_column_header_row = false;
                    // The line is no longer the names: it goes back to the header.
                    if recipe.main_until_marker.is_some() {
                        recipe.header_layout.subheader_lines += 1;
                    } else {
                        recipe.header_layout.main_header_lines += 1;
                    }
                }
            }
            AutoWhat::LongName => roles.long_name = None,
            AutoWhat::Units => {
                roles.unit = None;
                recipe.strip_unit_brackets = false;
            }
            AutoWhat::SameLine => {
                roles.same_line_name_unit = false;
                recipe.name_unit_slash = false;
            }
            AutoWhat::DataEnd => recipe.data_end = DataEnd::None,
            AutoWhat::KeepFooter => recipe.keep_footer_metadata = false,
            AutoWhat::NameSource => recipe.name_source = NameSource::Filename,
            AutoWhat::Prop(prop) => recipe.header_props.retain(|p| p != prop),
        }
    }
}

// -- reading the file ---------------------------------------------------------

fn looks_binary(bytes: &[u8]) -> bool {
    let utf16 = bytes.starts_with(&[0xFF, 0xFE]) || bytes.starts_with(&[0xFE, 0xFF]);
    if !utf16 {
        let head = &bytes[..bytes.len().min(4096)];
        // Padding NULs in a text header are text (Avantes); any other NUL is not.
        if head.contains(&0) && !crate::decode::has_sparse_nuls(bytes) {
            return true;
        }
        let control = head
            .iter()
            .filter(|&&b| b != 0 && b < 0x20 && !matches!(b, b'\t' | b'\r' | b'\n' | 0x0C | 0x1A))
            .count();
        if control * 100 > head.len() * 5 {
            return true;
        }
    }
    crate::decode::decode_text(bytes).is_err()
}

fn detect_encoding_choice(bytes: &[u8]) -> Encoding {
    if bytes.starts_with(&[0xFF, 0xFE]) {
        return Encoding::Utf16Le;
    }
    if bytes.starts_with(&[0xFE, 0xFF]) {
        return Encoding::Utf16Be;
    }
    let body = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(bytes);
    if std::str::from_utf8(body).is_err() && !body.utf8_chunks().any(|c| !c.valid().is_ascii()) {
        Encoding::Windows1252
    } else {
        Encoding::Auto
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Sep {
    Semi,
    Tab,
    Comma,
    Space,
}

/// The fields of a line, lazily (no allocation).
enum FieldIter<'a> {
    Split(std::str::Split<'a, char>),
    Whitespace(std::str::SplitWhitespace<'a>),
}

impl<'a> Iterator for FieldIter<'a> {
    type Item = &'a str;

    fn next(&mut self) -> Option<&'a str> {
        match self {
            Self::Split(it) => it.next(),
            Self::Whitespace(it) => it.next(),
        }
    }
}

fn field_iter(line: &str, sep: Sep) -> FieldIter<'_> {
    match sep {
        Sep::Semi => FieldIter::Split(line.split(';')),
        Sep::Tab => FieldIter::Split(line.split('\t')),
        Sep::Comma => FieldIter::Split(line.split(',')),
        Sep::Space => FieldIter::Whitespace(line.split_whitespace()),
    }
}

fn fields(line: &str, sep: Sep) -> Vec<&str> {
    field_iter(line, sep).collect()
}

fn is_num(numbers: &NumberFormat, token: &str) -> bool {
    let token = token.trim();
    !token.is_empty() && token.bytes().any(|b| b.is_ascii_digit()) && numbers.is_numeric(token)
}

/// Tokens that mean "no value" inside the data (plate readers, spreadsheets,
/// instrument software), compared case-insensitively. A cell holding one of
/// them counts as an empty cell for the detector, and the recipe reads it as
/// missing. Words that could be a real value or a unit are left out on purpose
/// (`null`, `none`, `open`, `ok`, `low`, `high`). `-` and `*` are kept: a
/// cell that is only a dash or stars is never a name.
const SENTINELS: [&str; 19] = [
    "overflow",
    "ovrflw",
    "over",
    "underflow",
    "nan",
    "n/a",
    "na",
    "#n/a",
    "---",
    "--",
    "-",
    "*",
    "**",
    "***",
    "****",
    "invalid",
    "error",
    "sat",
    "saturated",
];

/// At most this many distinct sentinel spellings go into the recipe.
const MAX_MISSING_TOKENS: usize = 8;

fn is_sentinel(token: &str) -> bool {
    let token = token.trim();
    SENTINELS.iter().any(|s| token.eq_ignore_ascii_case(s))
}

/// A sentinel that appeared in the data run, as spelt in the file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MissingToken {
    /// First spelling seen (the recipe compares case-insensitively).
    pub token: String,
    /// Occurrences over the data run.
    pub count: usize,
    /// 1-based file line of the first occurrence.
    pub first_line: usize,
}

/// What the file itself stated about its separator and decimal (`#DECIMAL:COMMA`,
/// `#SEPARATOR:SEMICOLON`) and the detector believed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatedFormat {
    pub separator: Option<SepKind>,
    /// `Some(true)` for a decimal comma, `Some(false)` for a decimal point.
    pub comma: Option<bool>,
    /// 1-based file line of the first of the stating lines.
    pub line: usize,
}

/// A data row: every non-empty field is a number or a sentinel, one at least is
/// a number, and the first field is a number, or a sentinel when at least half
/// of the other fields are numbers. Returns the number of fields.
#[cfg(test)]
fn numeric_row(numbers: &NumberFormat, line: &str, sep: Sep) -> Option<usize> {
    numeric_row_with(numbers, line, sep, false)
}

/// [`numeric_row`]; `lenient` (a plate reader) accepts a sentinel
/// first field with a single real number after it (`Overflow;Overflow;5`).
fn numeric_row_with(numbers: &NumberFormat, line: &str, sep: Sep, lenient: bool) -> Option<usize> {
    let mut it = field_iter(line, sep);
    let first = it.next()?;
    let first_is_num = is_num(numbers, first);
    if !first_is_num && !(is_sentinel(first)) {
        return None;
    }
    let mut real = usize::from(first_is_num);
    let mut len = 1usize;
    for t in it {
        len += 1;
        if is_num(numbers, t) {
            real += 1;
        } else if !t.trim().is_empty() && !is_sentinel(t) {
            return None;
        }
    }
    if real == 0 {
        return None;
    }
    // A sentinel first field needs half of the others to be real numbers.
    (first_is_num || lenient || real * 2 >= len - 1).then_some(len)
}

/// Non-empty fields, none a number, at least one not a sentinel: a names row.
fn all_text(numbers: &NumberFormat, f: &[&str]) -> bool {
    f.iter().any(|t| !t.trim().is_empty() && !is_sentinel(t))
        && f.iter().all(|t| !is_num(numbers, t))
}

/// The sentinels held by the data rows `start..end`.
fn missing_in_run(
    numbers: &NumberFormat,
    raw: &[&str],
    sep: Sep,
    start: usize,
    end: usize,
) -> Vec<MissingToken> {
    let mut found: Vec<MissingToken> = Vec::new();
    for (i, line) in raw.iter().enumerate().take(end).skip(start) {
        for t in fields(line, sep) {
            if is_num(numbers, t) || t.trim().is_empty() || !is_sentinel(t) {
                continue;
            }
            let t = t.trim();
            match found.iter().position(|m| m.token.eq_ignore_ascii_case(t)) {
                Some(k) => found[k].count += 1,
                None if found.len() < MAX_MISSING_TOKENS => found.push(MissingToken {
                    token: t.to_string(),
                    count: 1,
                    first_line: i + 1,
                }),
                None => {}
            }
        }
    }
    found
}

/// Reads the `DECIMAL` and `SEPARATOR` keys a file states in its first lines
/// (`#DECIMAL:COMMA`, `SEPARATOR;TAB`, the padded `#DECIMAL:   ;COMMA   ;;;`).
/// Returns (separator, comma, 1-based line of the first stating line).
fn stated_format(raw: &[&str], first: usize) -> Option<(Option<Sep>, Option<bool>, usize)> {
    let mut sep = None;
    let mut comma = None;
    let mut line_no = None;
    for (i, line) in raw.iter().enumerate().take(first + 100).skip(first) {
        let Some((key, rest)) = split_kv(line) else {
            continue;
        };
        let Some(value) = rest.split(';').map(str::trim).find(|v| !v.is_empty()) else {
            continue;
        };
        let value = value.to_ascii_uppercase();
        match key.to_ascii_uppercase().as_str() {
            "DECIMAL" if comma.is_none() => {
                comma = match value.as_str() {
                    "COMMA" => Some(true),
                    "POINT" | "DOT" | "PERIOD" => Some(false),
                    _ => None,
                };
                if comma.is_some() {
                    line_no.get_or_insert(i + 1);
                }
            }
            "SEPARATOR" if sep.is_none() => {
                sep = match value.as_str() {
                    "SEMICOLON" => Some(Sep::Semi),
                    "COMMA" => Some(Sep::Comma),
                    "TAB" => Some(Sep::Tab),
                    "SPACE" | "BLANK" => Some(Sep::Space),
                    _ => None,
                };
                if sep.is_some() {
                    line_no.get_or_insert(i + 1);
                }
            }
            _ => {}
        }
    }
    line_no.map(|l| (sep, comma, l))
}

fn is_marker_line(line: &str) -> bool {
    let t = line.trim();
    t.len() >= 3
        && t.chars().next().is_some_and(|c| c.is_ascii_uppercase())
        && t.chars()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
}

struct Format {
    sep: Sep,
    comma: bool,
    n: usize,
    start: usize,
    end: usize,
}

const CANDIDATES: [(Sep, bool); 7] = [
    (Sep::Semi, false),
    (Sep::Semi, true),
    (Sep::Tab, false),
    (Sep::Tab, true),
    (Sep::Comma, false),
    (Sep::Space, false),
    (Sep::Space, true),
];

/// Fewest rows a run needs for a stated format to be believed.
const STATED_MIN_ROWS: usize = 3;

/// Best run of data rows. A separator and decimal stated by the file are tried
/// first and kept when they give a run of at least [`STATED_MIN_ROWS`] rows;
/// otherwise the search is the plain one over all candidates (a header that
/// lies must not break a readable file).
fn best_format(
    raw: &[&str],
    first: usize,
    stated: Option<(Option<Sep>, Option<bool>)>,
    lenient: bool,
) -> (Option<Format>, bool) {
    if let Some((sep, comma)) = stated {
        let wanted: Vec<(Sep, bool)> = CANDIDATES
            .iter()
            .copied()
            .filter(|&(s, c)| sep.is_none_or(|w| w == s) && comma.is_none_or(|w| w == c))
            .collect();
        if let Some(f) = best_format_over(raw, first, &wanted, None, lenient)
            .filter(|f| f.end - f.start >= STATED_MIN_ROWS)
        {
            return (Some(f), true);
        }
    }
    (
        best_format_over(raw, first, &CANDIDATES, None, lenient),
        false,
    )
}

/// Best run of data rows over the candidates.
fn best_format_over(
    raw: &[&str],
    first: usize,
    candidates: &[(Sep, bool)],
    thousands: Option<char>,
    lenient: bool,
) -> Option<Format> {
    let window_end = raw.len().min(first + 3000);
    let mut best: Option<(usize, Format)> = None;
    for single in [false, true] {
        for &(sep, comma) in candidates {
            if single && sep != Sep::Space {
                continue;
            }
            let numbers = NumberFormat::new(
                if comma {
                    DecimalStyle::Comma
                } else {
                    DecimalStyle::Dot
                },
                thousands,
                &[],
                std::iter::empty(),
            );
            let mut run: Option<(usize, usize)> = None; // (start, n)
            let mut consider = |start: usize, end: usize, n: usize| {
                if end > start
                    && ((n >= 2) != single)
                    && best.as_ref().is_none_or(|b| end - start > b.0)
                {
                    best = Some((
                        end - start,
                        Format {
                            sep,
                            comma,
                            n,
                            start,
                            end,
                        },
                    ));
                }
            };
            for (i, line) in raw.iter().enumerate().take(window_end).skip(first) {
                let n = numeric_row_with(&numbers, line, sep, lenient);
                match (run, n) {
                    (Some((_, rn)), Some(n)) if rn == n => {}
                    (Some((s, rn)), _) => {
                        consider(s, i, rn);
                        run = n.map(|n| (i, n));
                    }
                    (None, Some(n)) => run = Some((i, n)),
                    (None, None) => {}
                }
            }
            if let Some((s, rn)) = run {
                let mut end = window_end;
                while end < raw.len()
                    && numeric_row_with(&numbers, raw[end], sep, lenient) == Some(rn)
                {
                    end += 1;
                }
                consider(s, end, rn);
            }
        }
        if best.is_some() {
            break;
        }
    }
    best.map(|(_, f)| f)
}

fn family_of(raw: &[&str], first: usize, has_xydata: bool) -> (Option<Family>, String) {
    let head = &raw[first..raw.len().min(first + 80)];
    for line in head {
        if let Some((k, v)) = split_kv(line) {
            if k.eq_ignore_ascii_case("ORIGIN") && v.eq_ignore_ascii_case("JASCO") {
                return (Some(Family::Jasco), line.trim().to_string());
            }
        }
    }
    if has_xydata {
        return (Some(Family::Jasco), "XYDATA".to_string());
    }
    let hashes = head
        .iter()
        .filter(|l| l.trim_start().starts_with("##"))
        .count();
    if hashes >= 2 {
        let line = head
            .iter()
            .find(|l| l.trim_start().starts_with("##"))
            .map_or("##", |l| l.trim());
        let key = line.split(['=', ':']).next().unwrap_or("##").trim();
        return (Some(Family::Jcamp), key.to_string());
    }
    (None, String::new())
}

/// Number of lines of the main header in `bytes` when it runs up to the
/// recipe's marker (so that the recipe can be kept in step with the file
/// shown). `None` without a marker, or when the file does not hold it.
pub fn marker_main_lines(bytes: &[u8], recipe: &ImportRecipe) -> Option<usize> {
    let marker = recipe.main_until_marker.as_ref()?.trim().to_lowercase();
    let text = crate::decode::decode_with(bytes, recipe.encoding).ok()?;
    let raw = crate::layout::split_lines(&text);
    let first = raw.iter().position(|l| !l.trim().is_empty())?;
    (first..raw.len())
        .find(|&i| raw[i].trim().to_lowercase() == marker)
        .map(|i| i + 1 - first)
}

/// Reads `bytes` and proposes the whole configuration. A JCAMP-DX file
/// ([`crate::jcamp::looks_jcamp`]) with decoded points is read by its own
/// reader (`recipe.format = Jcamp`) and is never refused.
pub fn sniff(bytes: &[u8], file_name: &str) -> Result<Sniff, SniffRefusal> {
    let table = sniff_table(bytes, file_name);
    if matches!(table, Err(SniffRefusal::Binary | SniffRefusal::Empty)) {
        return table;
    }
    let encoding = detect_encoding_choice(bytes);
    let Ok(text) = crate::decode::decode_with(bytes, encoding) else {
        return table;
    };
    // NMR is out of scope: refused before the table path can read part of it.
    if crate::jcamp::starts_with_title(&text)
        && crate::jcamp::mentions_nmr(&text)
        && crate::jcamp::read(&text)
            .blocks
            .iter()
            .all(|b| b.points() == 0)
    {
        return Err(SniffRefusal::Nmr);
    }
    if !crate::jcamp::looks_jcamp(&text) {
        return table;
    }
    let file = crate::jcamp::read(&text);
    if file.blocks.iter().all(|b| b.points() == 0) {
        // Recognised, but nothing decoded: the table detector keeps the
        // file, with what the reader could not read as doubts.
        return table.map(|mut s| {
            for (index, block) in file.blocks.iter().enumerate() {
                for issue in &block.issues {
                    if s.doubts.len() < MAX_DOUBT_LINES {
                        s.doubts.push(Doubt::Jcamp {
                            block: index,
                            issue: issue.clone(),
                        });
                    }
                }
            }
            s
        });
    }
    let mut sniff = apply_jcamp(table.ok(), &file, &text, encoding);
    if file.nmr_blocks > 0 && sniff.doubts.len() < MAX_DOUBT_LINES {
        sniff.doubts.push(Doubt::Jcamp {
            block: 0,
            issue: crate::jcamp::JcampIssue::NmrLeftOut {
                blocks: file.nmr_blocks,
            },
        });
    }
    Ok(sniff)
}

/// The proposal for a JCAMP-DX file: the table proposal (when the table
/// detector found one) adjusted, or a minimal one built on the header.
fn apply_jcamp(
    table: Option<Sniff>,
    file: &crate::jcamp::JcampFile,
    text: &str,
    encoding: Encoding,
) -> Sniff {
    use crate::jcamp::{JcampIssue, JcampSummary};
    let raw = crate::layout::split_lines(text);
    let first = raw.iter().position(|l| !l.trim().is_empty()).unwrap_or(0);
    let spectra: Vec<(usize, &crate::jcamp::JcampBlock)> = file
        .blocks
        .iter()
        .enumerate()
        .filter(|(_, b)| b.points() > 0)
        .collect();
    let lead = spectra[0].1;
    let mut sniff = table.unwrap_or_else(|| {
        // No table found (compressed values): the header runs up to the label
        // line of the first data table.
        let header = lead.first_line.saturating_sub(1).saturating_sub(first);
        let mut recipe = ImportRecipe::new("Auto");
        recipe.encoding = encoding;
        recipe.header_layout = HeaderLayout {
            main_header_lines: header,
            subheader_lines: 0,
            auto_detect: false,
        };
        let mut choices = Vec::new();
        if encoding != Encoding::Auto {
            choices.push(AutoChoice {
                what: AutoWhat::Encoding,
                lines: vec![],
                why: Why::Encoding(encoding),
            });
        }
        choices.push(AutoChoice {
            what: AutoWhat::DataStart,
            lines: vec![lead.first_line],
            why: Why::DataStart {
                line: lead.first_line,
                rows: lead.points(),
                columns: 2,
            },
        });
        Sniff {
            recipe,
            choices,
            family: None,
            signature: String::new(),
            kind: jcamp_kind(),
            checks: Vec::new(),
            rows: 0,
            columns: 2,
            missing: Vec::new(),
            stated: None,
            blocks: Vec::new(),
            doubts: Vec::new(),
            jcamp: None,
            instrument: None,
            thousands: None,
        }
    });
    sniff.instrument = crate::instrument::detect(&raw, first, Some(Family::Jcamp));
    sniff.recipe.format = crate::recipe::FileFormat::Jcamp;
    sniff.recipe.missing_tokens.clear();
    sniff.missing.clear();
    // The sheet takes the title of its block.
    sniff.choices.retain(|c| c.what != AutoWhat::NameSource);
    let title_line = raw
        .iter()
        .take(MAX_SCAN_LINES)
        .position(|l| crate::jcamp::record_of(l).is_some_and(|(label, _)| label == "TITLE"));
    if let Some(idx) = title_line {
        sniff.recipe.name_source = NameSource::HeaderKey("TITLE".to_string());
        sniff.choices.push(AutoChoice {
            what: AutoWhat::NameSource,
            lines: vec![idx + 1],
            why: Why::NameFromKey {
                key: "TITLE".into(),
                line: idx + 1,
            },
        });
    }
    if !matches!(sniff.family, Some(Family::Jcamp)) {
        sniff.signature = "##TITLE".to_string();
    }
    sniff.family = Some(Family::Jcamp);
    sniff.kind = jcamp_kind();
    sniff.rows = lead.points();
    sniff.columns = 2;
    sniff.checks = lead
        .npoints
        .map(|declared| Check::NPoints {
            declared,
            read: lead.points(),
        })
        .into_iter()
        .collect();
    sniff.blocks = spectra
        .iter()
        .take(MAX_BLOCKS)
        .map(|(_, b)| DataBlock {
            first_line: b.first_line,
            last_line: b.last_line,
            rows: b.points(),
            title: Some(b.title.chars().take(80).collect()).filter(|t: &String| !t.is_empty()),
        })
        .collect();
    let mut doubts = Vec::new();
    if spectra.len() >= 2 {
        doubts.push(Doubt::SeveralBlocks {
            count: spectra.len(),
            rows_total: spectra.iter().map(|(_, b)| b.points()).sum(),
            split: true,
        });
    }
    let mut issues = Vec::new();
    for (index, block) in file.blocks.iter().enumerate() {
        for issue in &block.issues {
            // The first spectrum's NPOINTS is a check, not a doubt.
            let is_lead_count =
                index == spectra[0].0 && matches!(issue, JcampIssue::NPointsMismatch { .. });
            if issues.len() < MAX_DOUBT_LINES {
                issues.push(issue.clone());
                if !is_lead_count {
                    doubts.push(Doubt::Jcamp {
                        block: index,
                        issue: issue.clone(),
                    });
                }
            }
        }
    }
    sniff.doubts = doubts;
    sniff.jcamp = Some(JcampSummary {
        blocks: spectra.len(),
        form: lead.form,
        issues,
    });
    sniff
}

fn jcamp_kind() -> KindKey {
    KindKey {
        family: Some(Family::Jcamp),
        sep: SepKind::None,
        comma: false,
        columns: 2,
        roles: 0,
        header_len: None,
        end: None,
    }
}

fn sniff_table(bytes: &[u8], _file_name: &str) -> Result<Sniff, SniffRefusal> {
    if bytes.iter().all(u8::is_ascii_whitespace) {
        return Err(SniffRefusal::Empty);
    }
    if looks_binary(bytes) {
        return Err(SniffRefusal::Binary);
    }
    let encoding = detect_encoding_choice(bytes);
    let text = crate::decode::decode_with(bytes, encoding).map_err(|_| SniffRefusal::Binary)?;
    let raw = crate::layout::split_lines(&text);
    let first = raw
        .iter()
        .position(|l| !l.trim().is_empty())
        .ok_or(SniffRefusal::Empty)?;
    let stated_keys = stated_format(&raw, first);
    let lenient = crate::instrument::detect(&raw, first, None)
        .is_some_and(|m| m.instrument == Instrument::PlateReader);
    let (plain, plain_kept) = best_format(
        &raw,
        first,
        stated_keys.map(|(sep, comma, _)| (sep, comma)),
        lenient,
    );
    // Grouped numbers (`1,234.5`) only count when they read more rows than
    // the plain reading does.
    let (format, stated_kept, thousands) =
        match pick_thousands(&raw, first, plain.as_ref(), stated_keys, lenient) {
            Some((f, guess)) => (Some(f), false, Some(guess)),
            None => (plain, plain_kept, None),
        };
    let format = format.ok_or(SniffRefusal::NoNumbers)?;
    let Format {
        sep,
        comma,
        n,
        start: data_idx,
        end: run_end,
    } = format;
    let numbers = NumberFormat::new(
        if comma {
            DecimalStyle::Comma
        } else {
            DecimalStyle::Dot
        },
        thousands.map(|g| g.ch),
        &[],
        std::iter::empty(),
    );
    let sep_kind = if n < 2 && sep == Sep::Space {
        SepKind::None
    } else {
        match sep {
            Sep::Semi => SepKind::Semicolon,
            Sep::Tab => SepKind::Tab,
            Sep::Comma => SepKind::Comma,
            Sep::Space => SepKind::Whitespace,
        }
    };
    let sep_of = |sep: Sep| match sep {
        Sep::Semi => SepKind::Semicolon,
        Sep::Tab => SepKind::Tab,
        Sep::Comma => SepKind::Comma,
        Sep::Space => SepKind::Whitespace,
    };
    let stated = stated_keys
        .filter(|_| stated_kept)
        .map(|(s, c, line)| StatedFormat {
            separator: s.map(sep_of),
            comma: c,
            line,
        });
    let inp = Input {
        raw: &raw,
        first,
        encoding,
        sep,
        comma,
        n,
        numbers: &numbers,
        sep_kind,
        stated: &stated,
        thousands,
        lenient,
    };

    // Every run of data rows of the same width, over the whole file.
    let (runs, cut) = find_runs(&inp, (data_idx, run_end));
    let blocks = blocks_of(&inp, &runs);
    let mut doubts: Vec<Doubt> = Vec::new();
    let mut chosen: Option<Sniff> = None;
    if runs.len() >= 2 {
        let rows_total: usize = runs.iter().map(|&(s, e)| e - s).sum();
        let interrupted = text_gaps(&inp, &runs).filter(|_| !cut);
        if let Some(gap_lines) = interrupted {
            if let Some(s) = try_text_rows(bytes, &inp, &runs, rows_total) {
                doubts.push(Doubt::TextRowsInData {
                    count: gap_lines.len(),
                    lines: gap_lines
                        .iter()
                        .take(MAX_DOUBT_LINES)
                        .map(|i| i + 1)
                        .collect(),
                });
                chosen = Some(s);
            }
        }
        if chosen.is_none() {
            let split = if cut {
                None
            } else {
                try_split(bytes, &inp, &runs)
            };
            doubts.push(Doubt::SeveralBlocks {
                count: runs.len(),
                rows_total,
                split: split.is_some(),
            });
            chosen = split;
        }
    }
    let mut result = chosen
        .unwrap_or_else(|| assemble(&inp, data_idx, run_end, run_end - data_idx, false).sniff);
    if let Some(line) = comma_ambiguity(&raw, data_idx, run_end, sep, comma) {
        doubts.push(Doubt::DecimalOrSeparator { line });
    }
    result.blocks = blocks;
    result.doubts = doubts;
    Ok(result)
}

/// The thousands reading that fits the file, with its format. Tried only for
/// the separators a grouping char cannot be mistaken for (semicolon, tab,
/// whitespace) and only when the grouping appears in the first lines.
fn pick_thousands(
    raw: &[&str],
    first: usize,
    plain: Option<&Format>,
    stated: Option<(Option<Sep>, Option<bool>, usize)>,
    lenient: bool,
) -> Option<(Format, ThousandsGuess)> {
    let plain_rows = plain.map_or(0, |f| f.end - f.start);
    let stated_sep = stated.and_then(|(s, _, _)| s);
    let stated_comma = stated.and_then(|(_, c, _)| c);
    // `present` depends on the grouping char only: one answer per reading.
    let mut present: [Option<bool>; crate::thousands::READINGS.len()] =
        [None; crate::thousands::READINGS.len()];
    for sep in [Sep::Semi, Sep::Tab, Sep::Space] {
        if stated_sep.is_some_and(|s| s != sep) {
            continue;
        }
        for (k, &(ch, dec)) in crate::thousands::READINGS.iter().enumerate() {
            if (sep == Sep::Space && ch == ' ') || stated_comma.is_some_and(|c| c != (dec == ',')) {
                continue;
            }
            let found =
                *present[k].get_or_insert_with(|| crate::thousands::present(raw, first, ch));
            if !found {
                continue;
            }
            let Some(guess) =
                crate::thousands::evidence(raw, first, &|line| fields(line, sep), ch, dec)
            else {
                continue;
            };
            let Some(format) =
                best_format_over(raw, first, &[(sep, dec == ',')], Some(ch), lenient)
            else {
                continue;
            };
            let rows = format.end - format.start;
            if rows > plain_rows && rows >= STATED_MIN_ROWS {
                return Some((format, guess));
            }
        }
    }
    None
}

/// The names row (and its unit row) of a known layout, 0-based lines.
struct ForcedNames {
    names: usize,
    /// Avantes: the `[nm] ;[counts]` row under the names; Netzsch has none (the
    /// unit is in the name: `Temp./°C`).
    unit: Option<usize>,
    /// Netzsch: the tokens are `name/unit`.
    slash: bool,
}

/// Where the names and units sit in an Avantes or a Netzsch file.
/// The generic rules cannot see them (blank line before the data, no brackets
/// in the units, `name/unit` tokens).
fn instrument_names(
    raw: &[&str],
    first: usize,
    data_idx: usize,
    sep: Sep,
    n: usize,
    instrument: &Option<InstrumentMatch>,
) -> Option<ForcedNames> {
    if sep != Sep::Semi || n < 2 {
        return None;
    }
    match instrument.as_ref()?.instrument {
        Instrument::Avantes => {
            let names = (first..data_idx).rev().find(|&i| {
                let t = raw[i].trim_start();
                t.len() >= 4
                    && t.is_char_boundary(4)
                    && t[..4].eq_ignore_ascii_case("wave")
                    && fields(raw[i], sep).len() == n
            })?;
            let unit = (names + 1 < data_idx
                && raw[names + 1].trim_start().starts_with('[')
                && fields(raw[names + 1], sep).len() == n)
                .then_some(names + 1);
            // Only blank lines may sit between the unit row and the data.
            let after = unit.unwrap_or(names) + 1;
            raw[after..data_idx]
                .iter()
                .all(|l| l.trim().is_empty())
                .then_some(ForcedNames {
                    names,
                    unit,
                    slash: false,
                })
        }
        Instrument::Netzsch => {
            let names = data_idx.checked_sub(1).filter(|&i| i >= first)?;
            let t = raw[names].trim_start();
            (t.starts_with('#') && t.contains('/') && fields(raw[names], sep).len() == n).then_some(
                ForcedNames {
                    names,
                    unit: None,
                    slash: true,
                },
            )
        }
        _ => None,
    }
}

/// Most lines a [`Doubt::TextRowsInData`] lists.
const MAX_DOUBT_LINES: usize = 50;
/// Most lines the search for the blocks reads, and most blocks it keeps.
pub(crate) const MAX_SCAN_LINES: usize = 200_000;
const MAX_BLOCKS: usize = 1000;

/// A run of data rows (0-based, end exclusive) of the width `n`. A run of one
/// row is noise (a stray number in a footer); a one-column run needs ten.
/// The flag says the search was cut short (too many blocks, or too long a
/// file): the list is then only a start, and no recipe is built on it.
fn find_runs(inp: &Input<'_>, chosen: (usize, usize)) -> (Vec<(usize, usize)>, bool) {
    let min_rows = if inp.n == 1 { 10 } else { 2 };
    let limit = inp.raw.len().min(inp.first.saturating_add(MAX_SCAN_LINES));
    let mut runs: Vec<(usize, usize)> = Vec::new();
    let mut start: Option<usize> = None;
    let mut cut = limit < inp.raw.len();
    for i in inp.first..limit {
        let hit = numeric_row_with(inp.numbers, inp.raw[i], inp.sep, inp.lenient) == Some(inp.n);
        match (start, hit) {
            (None, true) => start = Some(i),
            (Some(s), false) => {
                if i - s >= min_rows {
                    runs.push((s, i));
                }
                start = None;
                if runs.len() >= MAX_BLOCKS {
                    cut = true;
                    break;
                }
            }
            _ => {}
        }
    }
    if let Some(s) = start.filter(|_| runs.len() < MAX_BLOCKS) {
        if limit - s >= min_rows {
            runs.push((s, limit));
        }
    }
    // The run the detector chose is always a block.
    if !runs.iter().any(|&(s, e)| s <= chosen.0 && chosen.1 <= e) {
        runs.push(chosen);
        runs.sort_unstable();
    }
    (runs, cut)
}

/// Longest title kept for a block, in characters.
const MAX_TITLE_CHARS: usize = 80;

fn blocks_of(inp: &Input<'_>, runs: &[(usize, usize)]) -> Vec<DataBlock> {
    let mut blocks = Vec::with_capacity(runs.len());
    let mut floor = inp.first;
    for &(s, e) in runs {
        let title = (floor..s).rev().map(|i| inp.raw[i]).find(|line| {
            !line.trim().is_empty()
                && fields(line, inp.sep).iter().any(|t| !t.trim().is_empty())
                && numeric_row_with(inp.numbers, line, inp.sep, inp.lenient).is_none()
        });
        blocks.push(DataBlock {
            first_line: s + 1,
            last_line: e,
            rows: e - s,
            title: title.map(|t| t.trim().chars().take(MAX_TITLE_CHARS).collect()),
        });
        floor = e;
    }
    blocks
}

/// When every gap between the runs is one or two lines of text (a non-empty
/// first field, no number), the 0-based lines of those gaps.
fn text_gaps(inp: &Input<'_>, runs: &[(usize, usize)]) -> Option<Vec<usize>> {
    let mut lines = Vec::new();
    for pair in runs.windows(2) {
        let (from, to) = (pair[0].1, pair[1].0);
        if !(1..=2).contains(&(to - from)) {
            return None;
        }
        for i in from..to {
            let line = inp.raw[i];
            let first_field = fields(line, inp.sep).first().copied().unwrap_or("");
            if first_field.trim().is_empty()
                || numeric_row_with(inp.numbers, line, inp.sep, inp.lenient).is_some()
            {
                return None;
            }
            lines.push(i);
        }
    }
    Some(lines)
}

/// Case (a): one table interrupted by text lines, read as one table. Checked by
/// reading it: the import must give exactly the rows of the blocks.
fn try_text_rows(
    bytes: &[u8],
    inp: &Input<'_>,
    runs: &[(usize, usize)],
    rows_total: usize,
) -> Option<Sniff> {
    let (start, end) = (runs[0].0, runs[runs.len() - 1].1);
    let sniff = assemble(inp, start, end, rows_total, true).sniff;
    let table = crate::parse::parse(bytes, &sniff.recipe).ok()?;
    (table.row_count() == rows_total).then_some(sniff)
}

/// Lines (trimmed, non-empty) that appear exactly once in the header and once
/// in every gap between the blocks, at the same distance above the first row
/// of their block. Nearest to the data first.
fn common_lines<'a>(
    raw: &[&'a str],
    first: usize,
    runs: &[(usize, usize)],
) -> Vec<(&'a str, usize)> {
    use std::collections::HashMap;
    // text -> (occurrences, distance above `block_start`) over `from..to`.
    let census = |from: usize, to: usize, block_start: usize| {
        let mut map: HashMap<&'a str, (usize, usize)> = HashMap::new();
        for (i, line) in raw.iter().enumerate().take(to).skip(from) {
            let text = line.trim();
            if text.is_empty() {
                continue;
            }
            let entry = map.entry(text).or_insert((0, block_start - i));
            entry.0 += 1;
        }
        map
    };
    let mut common: HashMap<&'a str, usize> = census(first, runs[0].0, runs[0].0)
        .into_iter()
        .filter(|(_, (count, _))| *count == 1)
        .map(|(text, (_, d))| (text, d))
        .collect();
    for pair in runs.windows(2) {
        if common.is_empty() {
            break;
        }
        let gap = census(pair[0].1, pair[1].0, pair[1].0);
        common = gap
            .into_iter()
            .filter(|(text, (count, d))| *count == 1 && common.get(text) == Some(d))
            .map(|(text, (_, d))| (text, d))
            .collect();
    }
    let mut common: Vec<(&str, usize)> = common.into_iter().collect();
    common.sort_unstable_by_key(|&(text, d)| (d, text));
    common
}

/// How a line looks in a header, to compare the lines above two blocks.
fn line_shape(inp: &Input<'_>, line: &str) -> (usize, bool, bool) {
    let f = fields(line, inp.sep);
    (
        f.len(),
        all_text(inp.numbers, &f),
        f.first().is_some_and(|t| t.trim().is_empty()),
    )
}

/// Case (b): the blocks repeat a header line. The recipe splits the file
/// before the topmost repeated line and ends its main header at the nearest
/// one. Checked by reading it: one table per block, with the block's rows;
/// otherwise `None` and the caller keeps the single pick.
fn try_split(bytes: &[u8], inp: &Input<'_>, runs: &[(usize, usize)]) -> Option<Sniff> {
    let common = common_lines(inp.raw, inp.first, runs);
    let (s0, e0) = runs[0];
    let mut a = assemble(inp, s0, e0, e0 - s0, false);
    // The nearest repeated line must sit above the role lines.
    let (_, d_near) = *common.iter().find(|&&(_, d)| s0 - d < a.top)?;
    let topmost = common.last()?.0;
    let near_idx = s0 - d_near;
    let new_sub_start = near_idx + 1;
    let r = &mut a.sniff.recipe;
    let delta = (inp.first + r.header_layout.main_header_lines) as i64 - new_sub_start as i64;
    // Do the role lines look the same above every block?
    let depth = s0 - a.top;
    let same_roles = (1..=depth).all(|dist| {
        let shape = line_shape(inp, inp.raw[s0 - dist]);
        runs[1..]
            .iter()
            .all(|&(s, _)| line_shape(inp, inp.raw[s - dist]) == shape)
    });
    let shifted = |range: Option<SubheaderLineRange>| -> Option<Option<SubheaderLineRange>> {
        match range {
            None => Some(None),
            Some(x) => {
                let from = i64::try_from(x.from).ok()? + delta;
                let to = i64::try_from(x.to).ok()? + delta;
                (from >= 1).then(|| {
                    Some(SubheaderLineRange {
                        from: from as usize,
                        to: to.max(from) as usize,
                    })
                })
            }
        }
    };
    let roles = r.subheader_roles;
    let moved = (|| {
        Some((
            shifted(roles.long_name)?,
            shifted(roles.unit)?,
            shifted(roles.comment)?,
            shifted(roles.x_prime)?,
        ))
    })();
    let keep_roles = same_roles && moved.is_some();
    r.main_until_marker = Some(inp.raw[near_idx].trim().to_string());
    r.header_layout.main_header_lines = near_idx + 1 - inp.first;
    if keep_roles {
        if let Some((long_name, unit, comment, x_prime)) = moved {
            r.subheader_roles.long_name = long_name;
            r.subheader_roles.unit = unit;
            r.subheader_roles.comment = comment;
            r.subheader_roles.x_prime = x_prime;
        }
        r.header_layout.subheader_lines = a.header_end - new_sub_start;
    } else {
        r.subheader_roles = ImportRecipe::new("").subheader_roles;
        r.has_column_header_row = false;
        r.header_layout.subheader_lines = s0 - new_sub_start;
        a.sniff.kind.roles = 0;
        a.sniff.choices.retain(|c| {
            !matches!(
                c.what,
                AutoWhat::Names
                    | AutoWhat::LongName
                    | AutoWhat::Units
                    | AutoWhat::SameLine
                    | AutoWhat::XPrime
            )
        });
    }
    a.sniff.kind.header_len = None;
    a.sniff.choices.retain(|c| c.what != AutoWhat::Marker);
    a.sniff.choices.push(AutoChoice {
        what: AutoWhat::Marker,
        lines: vec![near_idx + 1],
        why: Why::Marker {
            line: near_idx + 1,
            text: inp.raw[near_idx].trim().to_string(),
        },
    });
    a.sniff.recipe.block_split = BlockSplit::BeforeLine(topmost.to_string());
    let tables = crate::parse::parse_blocks(bytes, &a.sniff.recipe).ok()?;
    let right = tables.len() == runs.len()
        && tables
            .iter()
            .zip(runs)
            .all(|(table, &(s, e))| table.row_count() == e - s);
    right.then_some(a.sniff)
}

/// A comma separator with a dot decimal, where most rows also look like
/// decimal-comma numbers split by `", "` (`893,69, 2397,762`): the first such
/// data line (1-based).
fn comma_ambiguity(raw: &[&str], from: usize, to: usize, sep: Sep, comma: bool) -> Option<usize> {
    if sep != Sep::Comma || comma {
        return None;
    }
    let looks = |line: &str| {
        line.contains(", ")
            && line
                .as_bytes()
                .windows(3)
                .any(|w| w[0].is_ascii_digit() && w[1] == b',' && w[2].is_ascii_digit())
    };
    let mut hits = 0usize;
    let mut first_hit = None;
    for (i, line) in raw.iter().enumerate().take(to).skip(from) {
        if looks(line) {
            hits += 1;
            first_hit.get_or_insert(i + 1);
        }
    }
    first_hit.filter(|_| hits * 100 >= (to - from) * 80)
}

/// What [`assemble`] needs from the reading of the file.
struct Input<'a> {
    raw: &'a [&'a str],
    first: usize,
    encoding: Encoding,
    sep: Sep,
    comma: bool,
    n: usize,
    numbers: &'a NumberFormat,
    sep_kind: SepKind,
    stated: &'a Option<StatedFormat>,
    /// The thousands separator the detector guessed.
    thousands: Option<ThousandsGuess>,
    /// A plate reader: a sentinel first field needs one real number only.
    lenient: bool,
}

/// A proposal, and where its header ends (for the block step).
struct Assembled {
    sniff: Sniff,
    /// First line (0-based) of the lines that sit just above the data: the
    /// roles when there are some, else the end of the header.
    top: usize,
    /// End (exclusive, 0-based) of the header; the names row is after it.
    header_end: usize,
}

/// Builds the whole proposal for the data run `data_idx..run_end` (0-based);
/// `rows` numeric rows are counted in it (more than the lines when
/// `text_rows`, which reads text lines inside the run as comments).
fn assemble(
    inp: &Input<'_>,
    data_idx: usize,
    run_end: usize,
    rows: usize,
    text_rows: bool,
) -> Assembled {
    let Input {
        raw,
        first,
        encoding,
        sep,
        comma,
        n,
        numbers,
        sep_kind,
        stated,
        thousands,
        lenient: _,
    } = *inp;
    let has_xydata = raw[first..data_idx].iter().any(|l| l.trim() == "XYDATA");
    let (family, signature) = family_of(raw, first, has_xydata);
    let instrument = crate::instrument::detect(raw, first, family);
    let kind_of = |i: Instrument| instrument.as_ref().is_some_and(|m| m.instrument == i);
    let ocean = kind_of(Instrument::OceanOptics);

    let missing = missing_in_run(numbers, raw, sep, data_idx, run_end);

    let mut recipe = ImportRecipe::new("Auto");
    recipe.missing_tokens = missing.iter().map(|m| m.token.clone()).collect();
    recipe.encoding = encoding;
    recipe.thousands = thousands.map(|g| g.ch);
    recipe.separator = match sep_kind {
        SepKind::Semicolon => Separator::Semicolon,
        SepKind::Tab => Separator::Tab,
        SepKind::Comma => Separator::Comma,
        SepKind::Whitespace => Separator::Whitespace,
        SepKind::None => Separator::Auto,
    };
    recipe.decimal = if comma {
        DecimalStyle::Comma
    } else {
        DecimalStyle::Dot
    };
    let mut choices: Vec<AutoChoice> = Vec::new();
    let line_no = |idx: usize| idx + 1;
    if encoding != Encoding::Auto {
        choices.push(AutoChoice {
            what: AutoWhat::Encoding,
            lines: vec![],
            why: Why::Encoding(encoding),
        });
    }
    choices.push(AutoChoice {
        what: AutoWhat::Separator,
        lines: vec![line_no(data_idx)],
        why: Why::Separator {
            kind: sep_kind,
            columns: n,
        },
    });
    choices.push(AutoChoice {
        what: AutoWhat::Decimal,
        lines: vec![line_no(data_idx)],
        why: Why::Decimal { comma },
    });

    // The lines just above the data.
    let mut roles = recipe.subheader_roles;
    let mut role_top: Option<usize> = None; // first role line (sub-header)
    let mut names_row = false;
    let mut role_choices: Vec<AutoChoice> = Vec::new();
    let mut role_bits = 0u8;
    let known_family = matches!(family, Some(Family::Jasco | Family::Jcamp));
    if data_idx > first && n >= 1 {
        let l1 = data_idx - 1;
        let f1 = fields(raw[l1], sep);
        if f1.len() == n && n >= 2 {
            let rest_numeric = f1[1..].iter().all(|t| is_num(numbers, t));
            if f1[0].trim().is_empty() && rest_numeric {
                roles.x_prime = Some(SubheaderLineRange::single(1));
                role_top = Some(l1);
                role_bits |= 1;
                role_choices.push(AutoChoice {
                    what: AutoWhat::XPrime,
                    lines: vec![line_no(l1)],
                    why: Why::XPrime {
                        line: line_no(l1),
                        count: n - 1,
                    },
                });
            } else if !is_num(numbers, f1[0]) && !f1[0].trim().is_empty() && rest_numeric {
                roles.x_prime = Some(SubheaderLineRange::single(1));
                roles.long_name = Some(SubheaderLineRange::single(1));
                role_top = Some(l1);
                role_bits |= 1 | 4;
                role_choices.push(AutoChoice {
                    what: AutoWhat::LongName,
                    lines: vec![line_no(l1)],
                    why: Why::LongNameXPrime { line: line_no(l1) },
                });
                role_choices.push(AutoChoice {
                    what: AutoWhat::XPrime,
                    lines: vec![line_no(l1)],
                    why: Why::XPrime {
                        line: line_no(l1),
                        count: n - 1,
                    },
                });
            }
        }
        if role_top.is_none() && !known_family && f1.len() == n && all_text(numbers, &f1) {
            let upper = (l1 > first)
                .then(|| fields(raw[l1 - 1], sep))
                .filter(|f| f.len() == n && n >= 2 && all_text(numbers, f));
            // A sparse line (a one-cell title over many columns) is a title,
            // not a role row: the line under it is then the names row.
            let title_above = upper.as_ref().is_some_and(|f| {
                n >= 3 && f.iter().filter(|t| !t.trim().is_empty()).count() * 2 < n
            });
            let upper = upper.filter(|_| !title_above);
            let bracketed = !title_above && f1.iter().any(|t| split_name_unit(t).1.is_some());
            if upper.is_some() {
                roles.long_name = Some(SubheaderLineRange::single(1));
                roles.unit = Some(SubheaderLineRange { from: 2, to: 2 });
                role_top = Some(l1 - 1);
                role_bits |= 4 | 8;
                role_choices.push(AutoChoice {
                    what: AutoWhat::LongName,
                    lines: vec![line_no(l1 - 1)],
                    why: Why::LongName {
                        line: line_no(l1 - 1),
                    },
                });
                role_choices.push(AutoChoice {
                    what: AutoWhat::Units,
                    lines: vec![line_no(l1)],
                    why: Why::Units { line: line_no(l1) },
                });
            } else if bracketed {
                roles.long_name = Some(SubheaderLineRange::single(1));
                roles.same_line_name_unit = true;
                role_top = Some(l1);
                role_bits |= 4 | 16;
                role_choices.push(AutoChoice {
                    what: AutoWhat::LongName,
                    lines: vec![line_no(l1)],
                    why: Why::LongName { line: line_no(l1) },
                });
                role_choices.push(AutoChoice {
                    what: AutoWhat::SameLine,
                    lines: vec![line_no(l1)],
                    why: Why::SameLine { line: line_no(l1) },
                });
            } else {
                names_row = true;
                role_bits |= 2;
                role_choices.push(AutoChoice {
                    what: AutoWhat::Names,
                    lines: vec![line_no(l1)],
                    why: Why::Names { line: line_no(l1) },
                });
            }
        }
    }
    // The names and units of a known layout.
    if let Some(forced) = instrument_names(raw, first, data_idx, sep, n, &instrument) {
        roles = ImportRecipe::new("").subheader_roles;
        roles.long_name = Some(SubheaderLineRange::single(1));
        role_choices.clear();
        role_choices.push(AutoChoice {
            what: AutoWhat::LongName,
            lines: vec![line_no(forced.names)],
            why: Why::LongName {
                line: line_no(forced.names),
            },
        });
        role_bits = 4;
        if let Some(unit) = forced.unit {
            roles.unit = Some(SubheaderLineRange { from: 2, to: 2 });
            role_bits |= 8;
            role_choices.push(AutoChoice {
                what: AutoWhat::Units,
                lines: vec![line_no(unit)],
                why: Why::Units {
                    line: line_no(unit),
                },
            });
            recipe.strip_unit_brackets = true;
        }
        if forced.slash {
            roles.same_line_name_unit = true;
            role_bits |= 16;
            role_choices.push(AutoChoice {
                what: AutoWhat::SameLine,
                lines: vec![line_no(forced.names)],
                why: Why::SameLine {
                    line: line_no(forced.names),
                },
            });
            recipe.name_unit_slash = true;
        }
        role_top = Some(forced.names);
        names_row = false;
    }
    let header_end = data_idx - usize::from(names_row);
    let top = role_top.unwrap_or(header_end);
    // A lone capitals line right before the roles: the marker. An Ocean
    // Optics file has its own: the `>>>>>Begin ... Data<<<<<` line.
    let marker_idx = top.checked_sub(1).filter(|&m| {
        m >= first
            && (is_marker_line(raw[m]) || (ocean && crate::instrument::is_ocean_begin(raw[m])))
    });
    let (main, sub) = match marker_idx {
        Some(m) => (m + 1 - first, header_end - (m + 1)),
        None => (top - first, header_end - top),
    };
    recipe.main_until_marker = marker_idx.map(|m| raw[m].trim().to_string());
    recipe.header_layout = HeaderLayout {
        main_header_lines: main,
        subheader_lines: sub,
        auto_detect: false,
    };
    recipe.has_column_header_row = names_row;
    recipe.skip_text_rows = text_rows;
    let sub_start = first + main;
    let rel = |idx: usize| idx + 1 - sub_start;
    // Ranges were written for relative line 1 (and 2): rebase on the real position.
    let rebase = |r: Option<SubheaderLineRange>, base_line: usize| {
        r.map(|r| SubheaderLineRange {
            from: r.from + rel(base_line) - 1,
            to: r.to + rel(base_line) - 1,
        })
    };
    if let Some(t) = role_top {
        roles.long_name = rebase(roles.long_name, t);
        roles.unit = rebase(roles.unit, t);
        roles.x_prime = rebase(roles.x_prime, t);
    }
    recipe.subheader_roles = roles;

    let data_choice = AutoChoice {
        what: AutoWhat::DataStart,
        lines: vec![line_no(data_idx)],
        why: Why::DataStart {
            line: line_no(data_idx),
            rows,
            columns: n,
        },
    };
    choices.push(data_choice);
    if let Some(m) = marker_idx {
        choices.push(AutoChoice {
            what: AutoWhat::Marker,
            lines: vec![line_no(m)],
            why: Why::Marker {
                line: line_no(m),
                text: raw[m].trim().to_string(),
            },
        });
    }
    choices.extend(role_choices);

    // The end of the data and the footer.
    let rest = &raw[run_end.min(raw.len())..];
    // Without a footer the cut is still the first blank line: harmless, and
    // it lets files with and without a footer share one kind and recipe.
    let mut end_kind = Some(EndKind::BlankLine);
    recipe.data_end = DataEnd::FirstBlankLine;
    if rest.iter().any(|l| !l.trim().is_empty()) {
        let line = raw[run_end].trim();
        let (data_end, kind) = if line.is_empty() {
            (DataEnd::FirstBlankLine, EndKind::BlankLine)
        } else if line.starts_with("#####")
            || (line.starts_with('[') && line.ends_with(']'))
            || (ocean
                && (crate::instrument::is_ocean_end(line)
                    || crate::instrument::is_ocean_begin(line)))
        {
            (DataEnd::Marker(line.to_string()), EndKind::Marker)
        } else if text_rows {
            // `Auto` would cut at the first text line of the table.
            (DataEnd::LinesAfterHeader(run_end - data_idx), EndKind::Text)
        } else {
            (DataEnd::Auto, EndKind::Text)
        };
        recipe.data_end = data_end;
        end_kind = Some(kind);
        choices.push(AutoChoice {
            what: AutoWhat::DataEnd,
            lines: vec![line_no(run_end)],
            why: Why::DataEnd {
                kind,
                line: line_no(run_end),
            },
        });
        let kv_count = rest
            .iter()
            .filter(|l| split_kv(l).is_some() && !l.trim().starts_with("####"))
            .count();
        if kv_count > 0 {
            recipe.keep_footer_metadata = true;
            choices.push(AutoChoice {
                what: AutoWhat::KeepFooter,
                lines: vec![line_no(run_end)],
                why: Why::KeepFooter { count: kv_count },
            });
        }
    }

    // `key;value` lines of the header and the footer.
    let skip: BTreeSet<usize> = role_top
        .into_iter()
        .chain(marker_idx)
        .chain(names_row.then(|| data_idx - 1))
        .collect();
    let mut kv: Vec<(usize, String, String)> = Vec::new();
    for (idx, line) in raw.iter().enumerate() {
        let in_header = idx >= first && idx < header_end;
        let in_footer = idx >= run_end;
        if !(in_header || in_footer) || skip.contains(&idx) {
            continue;
        }
        if let Some((k, v)) = split_kv(line) {
            if line.trim().starts_with("####") {
                continue;
            }
            kv.push((idx, k.trim().to_string(), v.to_string()));
        }
    }
    let find = |key: &str| {
        let wanted = key.to_uppercase();
        kv.iter()
            .find(|(_, k, v)| crate::props::norm_key(k) == wanted && !v.is_empty())
            .cloned()
    };
    let add_prop = |recipe: &mut ImportRecipe,
                    choices: &mut Vec<AutoChoice>,
                    found: Option<(usize, String, String)>,
                    field: PropField,
                    target: PropTarget,
                    require_bracket: bool| {
        let Some((idx, key, value)) = found else {
            return false;
        };
        if require_bracket && split_name_unit(&value).1.is_none() {
            return false;
        }
        let prop = HeaderProp {
            key: key.clone(),
            field,
            target,
            short_unit: field == PropField::Units,
        };
        if prop_text(&prop, &value).is_none() {
            return false;
        }
        recipe.header_props.push(prop.clone());
        choices.push(AutoChoice {
            what: AutoWhat::Prop(prop),
            lines: vec![line_no(idx)],
            why: Why::Prop {
                line: line_no(idx),
                key,
                value,
            },
        });
        true
    };
    add_prop(
        &mut recipe,
        &mut choices,
        find("Horizontal axis"),
        PropField::LongName,
        PropTarget::X,
        false,
    );
    // Units: the header key (XUNITS, YUNITS) first; the footer axis only when
    // the key is missing.
    if !add_prop(
        &mut recipe,
        &mut choices,
        find("XUNITS"),
        PropField::Units,
        PropTarget::X,
        false,
    ) {
        add_prop(
            &mut recipe,
            &mut choices,
            find("Horizontal axis"),
            PropField::Units,
            PropTarget::X,
            true,
        );
    }
    if !add_prop(
        &mut recipe,
        &mut choices,
        find("YUNITS"),
        PropField::Units,
        PropTarget::Y,
        false,
    ) {
        add_prop(
            &mut recipe,
            &mut choices,
            find("Vertical axis"),
            PropField::Units,
            PropTarget::Y,
            false,
        );
    }
    add_prop(
        &mut recipe,
        &mut choices,
        find("TITLE"),
        PropField::LongName,
        PropTarget::Y,
        false,
    );
    if !add_prop(
        &mut recipe,
        &mut choices,
        find("Sample name"),
        PropField::Comment,
        PropTarget::Y,
        false,
    ) {
        add_prop(
            &mut recipe,
            &mut choices,
            find("DATA TYPE"),
            PropField::Comment,
            PropTarget::Y,
            false,
        );
    }
    // The sheet takes the title when `TITLE` sits in the header.
    if matches!(sep_kind, SepKind::Semicolon | SepKind::Tab | SepKind::Comma) {
        if let Some((idx, _, _)) = kv.iter().find(|(i, k, v)| {
            crate::props::norm_key(k) == "TITLE" && !v.is_empty() && *i < header_end
        }) {
            recipe.name_source = NameSource::HeaderKey("TITLE".to_string());
            choices.push(AutoChoice {
                what: AutoWhat::NameSource,
                lines: vec![line_no(*idx)],
                why: Why::NameFromKey {
                    key: "TITLE".into(),
                    line: line_no(*idx),
                },
            });
        }
    }

    let mut checks = Vec::new();
    if let Some((_, _, v)) = kv
        .iter()
        .find(|(i, k, _)| crate::props::norm_key(k) == "NPOINTS" && *i < header_end)
    {
        if let Ok(declared) = v.trim().parse::<i64>() {
            checks.push(Check::NPoints {
                declared,
                read: rows,
            });
        }
    }

    let family = family.or(if header_end == first {
        Some(Family::PlainTable)
    } else {
        None
    });
    recipe.name = "Auto".to_string();
    let kind = KindKey {
        family,
        sep: sep_kind,
        comma,
        columns: n,
        roles: role_bits,
        header_len: marker_idx.is_none().then_some(header_end - first),
        end: end_kind,
    };
    Assembled {
        sniff: Sniff {
            recipe,
            choices,
            family,
            signature,
            kind,
            checks,
            rows,
            columns: n,
            missing,
            stated: stated.clone(),
            blocks: Vec::new(),
            doubts: Vec::new(),
            jcamp: None,
            instrument,
            thousands,
        },
        top,
        header_end,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn public(name: &str) -> Vec<u8> {
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../test/import/");
        std::fs::read(format!("{dir}{name}")).unwrap()
    }

    /// The anonymized kinetic, rebuilt in the test: 2 header props, the
    /// `XYDATA` marker, an x' line, a blank line then a footer.
    fn kinetic() -> Vec<u8> {
        let mut s = String::new();
        for l in [
            "TITLE;demo_kinetique_40C",
            "DATA TYPE;ULTRAVIOLET SPECTRUM",
            "ORIGIN;JASCO",
            "OWNER;",
            "DATE;2026/01/15",
            "TIME;09:00:00",
            "SPECTROMETER/DATA SYSTEM;JASCO Corp., V-770, Rev. 1.00",
            "DELTAX;-1",
            "XUNITS;NANOMETERS",
            "YUNITS;ABSORBANCE",
            "FIRSTX;  800,0000",
            "LASTX;  797,0000",
            "NPOINTS;     4",
            "FIRSTY;     0,00058",
            "MAXY;     0,71623",
            "MINY;     0,00041",
            "XYDATA",
            ";0;14,9833;30",
        ] {
            s += l;
            s += "\n";
        }
        for (i, w) in [800, 799, 798, 797].iter().enumerate() {
            s += &format!("{w};0,00052{i};0,00073;0,00054\n");
        }
        s += "\n##### Extended Information\n[Comments]\nSample name;echantillon_demo\n";
        s += "[Detailed Information]\nHorizontal axis;Wavelength [nm]\nVertical axis;Abs\n";
        s.into_bytes()
    }

    #[test]
    fn the_kinetic_is_set_without_a_click() {
        let s = sniff(&kinetic(), "k.csv").unwrap();
        let r = &s.recipe;
        assert_eq!(r.separator, Separator::Semicolon);
        assert_eq!(r.decimal, DecimalStyle::Comma);
        assert_eq!(r.main_until_marker.as_deref(), Some("XYDATA"));
        assert_eq!(r.header_layout.main_header_lines, 17);
        assert_eq!(r.header_layout.subheader_lines, 1);
        assert_eq!(
            r.subheader_roles.x_prime,
            Some(SubheaderLineRange::single(1))
        );
        assert_eq!(r.data_end, DataEnd::FirstBlankLine);
        assert!(r.keep_footer_metadata);
        assert_eq!(r.name_source, NameSource::HeaderKey("TITLE".into()));
        assert_eq!(s.family, Some(Family::Jasco));
        assert_eq!(s.columns, 4);
        assert_eq!(s.rows, 4);
        assert_eq!(
            s.checks,
            vec![Check::NPoints {
                declared: 4,
                read: 4
            }]
        );
        let table = crate::parse(&kinetic(), r).unwrap();
        assert_eq!(table.row_count(), 4);
        assert_eq!(table.columns[2].x_prime, Some(14.9833));
        assert_eq!(
            table.columns[1].long_name.as_deref(),
            Some("demo_kinetique_40C")
        );
        assert_eq!(table.columns[1].unit.as_deref(), Some("Abs"));
        assert_eq!(table.columns[0].unit.as_deref(), Some("nm"));
        assert_eq!(table.columns[0].long_name.as_deref(), Some("Wavelength"));
        assert_eq!(
            table.columns[1].comment.as_deref(),
            Some("echantillon_demo")
        );
        assert!(table.warnings.is_empty());
    }

    #[test]
    fn undoing_x_prime_sends_its_line_back_to_the_header() {
        let s = sniff(&kinetic(), "k.csv").unwrap();
        let mut r = s.recipe.clone();
        AutoWhat::XPrime.undo(&mut r);
        assert!(!AutoWhat::XPrime.is_set(&r, &s.recipe));
        let table = crate::parse(&kinetic(), &r).unwrap();
        assert_eq!(table.row_count(), 4, "the x' line is not read as data");
        assert_eq!(table.columns[2].x_prime, None);
    }

    #[test]
    fn jasco_tab_with_the_title_in_the_header() {
        let s = sniff(&public("jasco_fluo_tab.txt"), "f").unwrap();
        assert_eq!(s.recipe.separator, Separator::Tab);
        assert_eq!(s.recipe.decimal, DecimalStyle::Comma);
        assert_eq!(s.recipe.main_until_marker.as_deref(), Some("XYDATA"));
        assert_eq!(s.rows, 6);
        let t = crate::parse(&public("jasco_fluo_tab.txt"), &s.recipe).unwrap();
        assert_eq!(t.columns[0].unit.as_deref(), Some("nm"));
        assert_eq!(t.columns[1].unit.as_deref(), Some("%T"));
        assert_eq!(
            t.columns[1].long_name.as_deref(),
            Some("Fixture_Jasco_Fluo")
        );
        assert!(s.checks.iter().all(Check::ok));
    }

    #[test]
    fn english_locale_reads_the_dot() {
        let s = sniff(&public("jasco_english_locale_dot.csv"), "f").unwrap();
        assert_eq!(s.recipe.decimal, DecimalStyle::Dot);
        assert_eq!(s.recipe.separator, Separator::Semicolon);
        assert_eq!(s.rows, 6);
    }

    #[test]
    fn the_point_count_mismatch_is_flagged() {
        let s = sniff(&public("jasco_npoints_mismatch.txt"), "f").unwrap();
        assert_eq!(
            s.checks,
            vec![Check::NPoints {
                declared: 99,
                read: 4
            }]
        );
        assert!(!s.checks[0].ok());
    }

    #[test]
    fn the_big_jasco_file_is_read_whole() {
        let bytes = public("JascoUV_vis.csv");
        let s = sniff(&bytes, "f").unwrap();
        assert_eq!(s.rows, 551);
        assert_eq!(s.recipe.encoding, Encoding::Windows1252);
        let t = crate::parse(&bytes, &s.recipe).unwrap();
        assert_eq!(t.row_count(), 551);
    }

    #[test]
    fn plain_tables_have_no_header() {
        for (name, sep, dec) in [
            (
                "analytikjena_nohead_semicolon.csv",
                Separator::Semicolon,
                DecimalStyle::Comma,
            ),
            ("rpe_nohead_tab.asc", Separator::Tab, DecimalStyle::Dot),
        ] {
            let s = sniff(&public(name), name).unwrap();
            assert_eq!(s.recipe.separator, sep, "{name}");
            assert_eq!(s.recipe.decimal, dec, "{name}");
            assert_eq!(s.recipe.header_layout.main_header_lines, 0);
            assert_eq!(s.rows, 20);
            assert_eq!(s.family, Some(Family::PlainTable));
        }
    }

    #[test]
    fn a_names_line_is_found() {
        let s = sniff(&public("generic_named_header.csv"), "f").unwrap();
        assert!(s.recipe.has_column_header_row);
        assert_eq!(s.recipe.separator, Separator::Comma);
        assert_eq!(s.rows, 30);
        let t = crate::parse(&public("generic_named_header.csv"), &s.recipe).unwrap();
        assert_eq!(t.columns[1].name.as_deref(), Some("Signal"));
    }

    #[test]
    fn the_wide_matrix_has_its_first_line_as_names_and_x_prime() {
        let bytes = public("irradiation_wide_matrix.txt");
        let s = sniff(&bytes, "f").unwrap();
        assert_eq!(s.recipe.separator, Separator::Tab);
        assert!(s.recipe.subheader_roles.x_prime.is_some());
        assert!(s.recipe.subheader_roles.long_name.is_some());
        let t = crate::parse(&bytes, &s.recipe).unwrap();
        assert_eq!(t.columns[3].x_prime, Some(2.0));
        assert_eq!(t.columns[0].x_prime, None);
    }

    #[test]
    fn the_jcamp_look_alike_reads_its_hash_lines() {
        let bytes = public("ir_jcamp_lookalike.txt");
        let s = sniff(&bytes, "f").unwrap();
        assert_eq!(s.family, Some(Family::Jcamp));
        assert_eq!(s.recipe.header_layout.main_header_lines, 4);
        let t = crate::parse(&bytes, &s.recipe).unwrap();
        assert_eq!(t.columns[0].unit.as_deref(), Some("cm⁻¹"));
        assert_eq!(t.columns[1].unit.as_deref(), Some("%T"));
        assert_eq!(t.columns[1].long_name.as_deref(), Some("FTIR Measurement"));
    }

    #[test]
    fn the_binary_file_is_refused() {
        assert_eq!(
            sniff(&public("proprietary_binary.dat"), "f").unwrap_err(),
            SniffRefusal::Binary
        );
        assert_eq!(sniff(b"", "f").unwrap_err(), SniffRefusal::Empty);
        assert_eq!(
            sniff(b"only words here\n", "f").unwrap_err(),
            SniffRefusal::NoNumbers
        );
    }

    // -- text sentinels, sparse NULs, stated format -----------------

    fn spectrum_rows() -> Vec<u8> {
        let mut s = String::from("Plate reader export\n\n[nm],A1,B1\n");
        s += "200,3.1,Overflow\n203,3.6913,Overflow\n206,Overflow,3.817\n";
        for k in 0..10 {
            s += &format!("{},1.{k},2.{k}\n", 209 + 3 * k);
        }
        s.into_bytes()
    }

    #[test]
    fn sentinels_in_the_first_rows_do_not_shift_the_data_start() {
        let bytes = spectrum_rows();
        let s = sniff(&bytes, "f").unwrap();
        // Lines 1-3 are the title, a blank and the names; data start on line 4.
        let start = s.choice(&AutoWhat::DataStart).unwrap();
        assert_eq!(start.lines, vec![4]);
        assert_eq!(s.rows, 13);
        assert!(
            s.recipe.subheader_roles.long_name.is_some(),
            "names row found"
        );
        assert_eq!(
            s.missing,
            vec![MissingToken {
                token: "Overflow".into(),
                count: 3,
                first_line: 4
            }]
        );
        assert_eq!(s.recipe.missing_tokens, vec!["Overflow".to_string()]);
        let t = crate::parse(&bytes, &s.recipe).unwrap();
        assert_eq!(t.row_count(), 13);
        assert!(t.columns[2].values[0].is_nan());
        assert!(t.columns[1].values[2].is_nan());
        assert_eq!(t.columns[1].values[1], 3.6913);
        assert!(t.warnings.is_empty());
    }

    #[test]
    fn nan_and_dashes_do_not_shift_the_start() {
        let bytes = b"t;a;b\n1;NaN;5\n2;3;---\n3;4;6\n4;nan;7\n";
        let s = sniff(bytes, "f").unwrap();
        assert_eq!(s.rows, 4);
        assert_eq!(s.choice(&AutoWhat::DataStart).unwrap().lines, vec![2]);
        let tokens: Vec<_> = s
            .missing
            .iter()
            .map(|m| (m.token.as_str(), m.count))
            .collect();
        assert_eq!(tokens, vec![("NaN", 2), ("---", 1)]);
        let t = crate::parse(bytes, &s.recipe).unwrap();
        assert!(t.columns[1].values[0].is_nan());
        assert!(t.columns[2].values[1].is_nan());
        assert_eq!(t.columns[1].values[1], 3.0);
        assert!(t.warnings.is_empty());
    }

    #[test]
    fn a_row_of_sentinels_only_is_not_a_data_row() {
        let n = NumberFormat::new(DecimalStyle::Dot, None, &[], std::iter::empty());
        assert_eq!(numeric_row(&n, "-;-;NaN", Sep::Semi), None);
        assert_eq!(numeric_row(&n, "NA;;", Sep::Semi), None);
        assert_eq!(numeric_row(&n, "NaN", Sep::Semi), None);
        assert_eq!(numeric_row(&n, "1;Overflow;x", Sep::Semi), None);
        assert_eq!(numeric_row(&n, "1;Overflow;", Sep::Semi), Some(3));
    }

    #[test]
    fn a_sentinel_first_field_needs_half_numbers() {
        let n = NumberFormat::new(DecimalStyle::Dot, None, &[], std::iter::empty());
        assert_eq!(numeric_row(&n, "NaN;1;2", Sep::Semi), Some(3));
        assert_eq!(numeric_row(&n, "NaN;1;NaN;NaN", Sep::Semi), None);
        assert_eq!(numeric_row(&n, "NaN;1;NaN", Sep::Semi), Some(3));
    }

    #[test]
    fn a_sentinel_is_not_a_names_row() {
        let n = NumberFormat::new(DecimalStyle::Dot, None, &[], std::iter::empty());
        assert!(!all_text(&n, &["-", "NaN"]));
        assert!(all_text(&n, &["Time", "NaN"]));
    }

    #[test]
    fn at_most_eight_spellings_are_kept() {
        let mut s = String::from("t;a\n");
        for (i, tok) in [
            "Over", "OVER", "over", "nan", "na", "n/a", "--", "-", "*", "**", "***",
        ]
        .iter()
        .enumerate()
        {
            s += &format!("{i};{tok}\n{i}.5;1\n");
        }
        let r = sniff(s.as_bytes(), "f").unwrap();
        assert_eq!(r.missing.len(), 8);
        assert_eq!(r.missing[0].token, "Over");
        assert_eq!(r.missing[0].count, 3, "case variants share one entry");
        assert_eq!(r.recipe.missing_tokens.len(), 8);
    }

    fn avantes() -> Vec<u8> {
        let mut b = Vec::new();
        b.extend_from_slice(
            b"Avantes export\nOperator;someone\nIntegration time;10 ms\nAveraging;5\n",
        );
        b.extend_from_slice(b"Data measured with spectrometer name: 0804016U1");
        b.extend(std::iter::repeat_n(0u8, 55));
        b.extend_from_slice(
            b"\nWave ;Dark ;Ref ;Sample ;Transmittance\n[nm] ;[counts] ;[counts] ;[counts] ;[%]\n",
        );
        for k in 0..200 {
            b.extend_from_slice(
                format!(
                    "{:.2};768.60;885.60;795.{k}0;23.333\n",
                    275.27 + f64::from(k)
                )
                .as_bytes(),
            );
        }
        b
    }

    #[test]
    fn sparse_nuls_in_a_header_line_are_text() {
        let bytes = avantes();
        let s = sniff(&bytes, "f").unwrap();
        assert_eq!(s.columns, 5);
        assert_eq!(s.rows, 200);
        assert_eq!(s.recipe.separator, Separator::Semicolon);
        assert!(
            s.recipe.subheader_roles.long_name.is_some(),
            "names row found"
        );
        let t = crate::parse(&bytes, &s.recipe).unwrap();
        assert_eq!(t.columns.len(), 5);
        assert_eq!(t.row_count(), 200);
        assert_eq!(t.columns[4].long_name.as_deref(), Some("Transmittance"));
        // The Avantes unit row is read without its brackets.
        assert_eq!(t.columns[4].unit.as_deref(), Some("%"));
        let shown = crate::preview(&bytes, &s.recipe).unwrap();
        assert!(shown
            .lines
            .iter()
            .all(|l| l.cells.as_ref().is_none_or(|c| c.len() == 5)));
    }

    #[test]
    fn nul_padding_is_stripped_from_key_values() {
        let (k, v) = split_kv("Spectrometer name: 0804016U1\0\0\0  ").unwrap();
        assert_eq!((k, v), ("Spectrometer name", "0804016U1"));
    }

    #[test]
    fn utf16_without_a_mark_and_dense_nuls_stay_refused() {
        let utf16: Vec<u8> = "1;2\n3;4\n5;6\n"
            .encode_utf16()
            .flat_map(u16::to_le_bytes)
            .collect();
        assert_eq!(sniff(&utf16, "f").unwrap_err(), SniffRefusal::Binary);
        // NULs on more than two lines are binary even when few.
        let mut spread = Vec::new();
        for _ in 0..4 {
            spread.extend_from_slice(b"1;2;3;4;5;6;7;8;9;10;11;12;13;14;15;16;17;18;19;20\0\n");
        }
        assert_eq!(sniff(&spread, "f").unwrap_err(), SniffRefusal::Binary);
        assert_eq!(
            sniff(&public("proprietary_binary.dat"), "f").unwrap_err(),
            SniffRefusal::Binary
        );
    }

    #[test]
    fn a_stated_comma_and_semicolon_are_believed() {
        let mut s =
            String::from("#DECIMAL:COMMA\n#SEPARATOR:SEMICOLON\n##Temp./\u{b0}C;Time/min;Mass/%\n");
        for k in 0..5 {
            s += &format!(" 2{k},14100;  0,0{k}000;99,98948\n");
        }
        let r = sniff(s.as_bytes(), "f").unwrap();
        assert_eq!(r.recipe.separator, Separator::Semicolon);
        assert_eq!(r.recipe.decimal, DecimalStyle::Comma);
        assert_eq!(r.rows, 5);
        assert_eq!(
            r.stated,
            Some(StatedFormat {
                separator: Some(SepKind::Semicolon),
                comma: Some(true),
                line: 1
            })
        );
    }

    #[test]
    fn the_padded_sta_form_is_read() {
        let pad = |k: &str, v: &str| format!("#{k}:{:<20};{:<36};;;\n", "", v);
        let mut s = pad("DECIMAL", "COMMA");
        s += &pad("SEPARATOR", "SEMICOLON");
        s += "#Temp./\u{b0}C;Time/min;DSC/(mW/mg)\n";
        for k in 0..4 {
            s += &format!("{k},5;{k},25;1,125\n");
        }
        let r = sniff(s.as_bytes(), "f").unwrap();
        assert_eq!(r.recipe.decimal, DecimalStyle::Comma);
        assert_eq!(r.recipe.separator, Separator::Semicolon);
        assert_eq!(r.stated.as_ref().and_then(|x| x.comma), Some(true));
        assert_eq!(r.rows, 4);
    }

    #[test]
    fn a_lying_header_falls_back_to_the_data() {
        let bytes = b"#SEPARATOR:TAB\n#DECIMAL:POINT\nt;a\n1,5;2,5\n3,5;4,5\n5,5;6,5\n";
        let r = sniff(bytes, "f").unwrap();
        assert_eq!(r.recipe.separator, Separator::Semicolon);
        assert_eq!(r.recipe.decimal, DecimalStyle::Comma);
        assert_eq!(r.rows, 3);
        assert_eq!(r.stated, None);
    }

    // -- several blocks in one file -----------------------------------

    /// A Pyris-like table: one table interrupted by one-line segment titles.
    fn titled_segments() -> Vec<u8> {
        let mut s = String::from("Sample run\n\tTime\tWeight\n");
        for (k, title) in [(0, None), (3, Some("1) Scan")), (6, Some("2) Scan"))] {
            if let Some(t) = title {
                s += &format!("{t}\n");
            }
            for j in k..k + 3 {
                s += &format!("\t{j}.5\t7.{j}\n");
            }
        }
        s += "\nCalibration note\n";
        s.into_bytes()
    }

    #[test]
    fn a_table_interrupted_by_titles_is_read_as_one() {
        let bytes = titled_segments();
        let r = sniff(&bytes, "f").unwrap();
        assert!(r.recipe.skip_text_rows);
        assert_eq!(r.recipe.data_end, DataEnd::FirstBlankLine);
        assert_eq!(r.rows, 9);
        assert_eq!(
            r.doubts,
            vec![Doubt::TextRowsInData {
                count: 2,
                lines: vec![6, 10]
            }]
        );
        assert_eq!(r.blocks.len(), 3);
        assert_eq!(r.blocks[1].title.as_deref(), Some("1) Scan"));
        let tables = crate::parse_blocks(&bytes, &r.recipe).unwrap();
        assert_eq!(tables.len(), 1);
        assert_eq!(tables[0].row_count(), 9);
        // Without the flag the titles would be data.
        let mut plain = r.recipe.clone();
        plain.skip_text_rows = false;
        assert_ne!(crate::parse(&bytes, &plain).unwrap().row_count(), 9);
    }

    #[test]
    fn titles_followed_by_a_text_footer_end_the_table_by_count() {
        let mut bytes = titled_segments();
        bytes.truncate(bytes.len() - "\nCalibration note\n".len());
        bytes.extend_from_slice(b"Calibration note\n");
        let r = sniff(&bytes, "f").unwrap();
        assert!(matches!(r.recipe.data_end, DataEnd::LinesAfterHeader(_)));
        assert_eq!(crate::parse(&bytes, &r.recipe).unwrap().row_count(), 9);
    }

    #[test]
    fn repeated_headers_split_the_file_into_tables() {
        let mut s = String::new();
        for (name, rows) in [("a", 3usize), ("b", 2), ("c", 4)] {
            s += &format!("# Region: {name}\n#\n# Axis: energy\n#\n# Cols: e c\n");
            for j in 0..rows {
                s += &format!("{j}.5 {j}0\n");
            }
            s += "\n";
        }
        let r = sniff(s.as_bytes(), "f").unwrap();
        assert_eq!(
            r.doubts,
            vec![Doubt::SeveralBlocks {
                count: 3,
                rows_total: 9,
                split: true
            }]
        );
        assert_eq!(
            r.recipe.block_split,
            BlockSplit::BeforeLine("# Axis: energy".into())
        );
        assert_eq!(r.recipe.main_until_marker.as_deref(), Some("# Cols: e c"));
        let sizes: Vec<usize> = crate::parse_blocks(s.as_bytes(), &r.recipe)
            .unwrap()
            .iter()
            .map(|t| t.row_count())
            .collect();
        assert_eq!(sizes, vec![3, 2, 4]);
        assert_eq!(r.blocks.len(), 3);
        assert!(!r.recipe.skip_text_rows);
    }

    #[test]
    fn stacked_matrices_are_flagged_not_split() {
        let mut s = String::from("GG2\n");
        for label in ["0s", "21s", "43s"] {
            s += &format!("{label},,,\n1,2,3,4\n5,6,7,8\n,,,\n,,,\n,,,\n");
        }
        let r = sniff(s.as_bytes(), "f").unwrap();
        assert_eq!(r.blocks.len(), 3);
        assert_eq!(r.blocks[1].title.as_deref(), Some("21s,,,"));
        assert_eq!(
            r.doubts,
            vec![Doubt::SeveralBlocks {
                count: 3,
                rows_total: 6,
                split: false
            }]
        );
        assert_eq!(r.recipe.block_split, BlockSplit::None);
        assert!(!r.recipe.skip_text_rows);
    }

    #[test]
    fn one_blank_line_inside_a_table_is_left_to_the_user() {
        let r = sniff(b"t,v\n1,2\n3,4\n\n5,6\n7,8\n", "f").unwrap();
        assert_eq!(r.blocks.len(), 2);
        assert_eq!(
            r.doubts,
            vec![Doubt::SeveralBlocks {
                count: 2,
                rows_total: 4,
                split: false
            }]
        );
        assert_eq!(r.recipe.block_split, BlockSplit::None);
    }

    #[test]
    fn a_repeated_line_at_another_distance_is_not_a_split_point() {
        let mut s = String::new();
        for (name, extra) in [("a", ""), ("b", "# Extra\n"), ("c", "")] {
            s += &format!("# Region: {name}\n# Axis: energy\n{extra}# Cols: e c\n");
            s += "1.5 10\n2.5 20\n\n";
        }
        let r = sniff(s.as_bytes(), "f").unwrap();
        assert_eq!(
            r.recipe.block_split,
            BlockSplit::BeforeLine("# Cols: e c".into())
        );
    }

    #[test]
    fn a_split_that_does_not_give_the_blocks_is_dropped() {
        // The footer line starts with the split text: the split would cut once more.
        let mut s = String::new();
        for name in ["a", "b", "c"] {
            s += &format!("# Region: {name}\n# Cols: e c\n1.5 10\n2.5 20\n# Cols: e c done\n\n");
        }
        let r = sniff(s.as_bytes(), "f").unwrap();
        assert_eq!(r.recipe.block_split, BlockSplit::None);
        assert_eq!(
            r.doubts,
            vec![Doubt::SeveralBlocks {
                count: 3,
                rows_total: 6,
                split: false
            }]
        );
    }

    #[test]
    fn long_gaps_and_numeric_gaps_are_not_titles() {
        // Three text lines between the blocks: too many to be titles.
        let r = sniff(b"1,2\n3,4\nx\ny\nz\n5,6\n7,8\n", "f").unwrap();
        assert!(!r.recipe.skip_text_rows);
        assert!(matches!(
            r.doubts[0],
            Doubt::SeveralBlocks { split: false, .. }
        ));
        // A number of another width between the blocks is not a title either.
        let r = sniff(b"1,2\n3,4\n9\n5,6\n7,8\n", "f").unwrap();
        assert!(!r.recipe.skip_text_rows);
        assert!(matches!(
            r.doubts[0],
            Doubt::SeveralBlocks { split: false, .. }
        ));
    }

    #[test]
    fn a_stray_pair_in_a_one_column_list_is_not_a_block() {
        let mut s = String::new();
        for k in 0..30 {
            s += &format!("{k}.5\n");
        }
        s += "note\n7\n8\n";
        let r = sniff(s.as_bytes(), "f").unwrap();
        assert_eq!(r.blocks.len(), 1);
        assert!(r.doubts.is_empty());
    }

    #[test]
    fn a_block_title_is_cut_to_eighty_characters() {
        let long = "T".repeat(120);
        let s = format!("{long}\n1,2\n3,4\n");
        let r = sniff(s.as_bytes(), "f").unwrap();
        assert_eq!(r.blocks[0].title.as_deref(), Some(&long[..80]));
    }

    #[test]
    fn a_line_repeated_in_the_gaps_or_the_header_is_not_a_split_point() {
        let cols = "# Cols: e c\n1.5 10\n2.5 20\n\n";
        // Once in the header, twice in each gap.
        let mut s = format!("# Region: a\n# Axis\n# X\n{cols}");
        for name in ["b", "c"] {
            s += &format!("# Region: {name}\n# Axis\n# Axis\n{cols}");
        }
        let r = sniff(s.as_bytes(), "f").unwrap();
        assert_eq!(
            r.recipe.block_split,
            BlockSplit::BeforeLine("# Cols: e c".into())
        );
        // Twice in the header, once in each gap.
        let mut s = format!("# Region: a\n# Axis\n# Axis\n{cols}");
        for name in ["b", "c"] {
            s += &format!("# Region: {name}\n# Axis\n# X\n{cols}");
        }
        let r = sniff(s.as_bytes(), "f").unwrap();
        assert_eq!(
            r.recipe.block_split,
            BlockSplit::BeforeLine("# Cols: e c".into())
        );
    }

    #[test]
    fn titles_that_the_import_would_not_skip_are_not_trusted() {
        // Nine spellings of "no value" as first field: the recipe keeps eight,
        // the ninth row would be read as a title and lost.
        let mut s = String::new();
        for t in [
            "overflow",
            "ovrflw",
            "over",
            "underflow",
            "nan",
            "n/a",
            "na",
            "#n/a",
            "invalid",
        ] {
            s += &format!("{t},1.5,2.5\n");
        }
        s += "segment\n3.5,4.5,5.5\n4.5,5.5,6.5\n";
        let r = sniff(s.as_bytes(), "f").unwrap();
        assert!(!r.recipe.skip_text_rows);
        assert!(matches!(
            r.doubts[0],
            Doubt::SeveralBlocks { split: false, .. }
        ));
    }

    #[test]
    fn a_single_block_raises_no_doubt() {
        let r = sniff(b"t,v\n1,2\n3,4\n5,6\n", "f").unwrap();
        assert_eq!(r.blocks.len(), 1);
        assert_eq!(r.blocks[0].first_line, 2);
        assert_eq!(r.blocks[0].last_line, 4);
        assert!(r.doubts.is_empty());
    }

    #[test]
    fn the_tuned_files_raise_no_doubt() {
        for name in [
            "analytikjena_nohead_semicolon.csv",
            "generic_named_header.csv",
            "ir_jcamp_lookalike.txt",
            "irradiation_wide_matrix.txt",
            "jasco_english_locale_dot.csv",
            "jasco_fluo_tab.txt",
            "jasco_npoints_mismatch.txt",
            "JascoUV_vis.csv",
            "jasco_uvvis_semicolon.csv",
            "rpe_nohead_tab.asc",
        ] {
            let r = sniff(&public(name), name).unwrap();
            assert!(r.doubts.is_empty(), "{name}: {:?}", r.doubts);
            assert_eq!(r.blocks.len(), 1, "{name}");
            assert!(!r.recipe.skip_text_rows, "{name}");
        }
        let r = sniff(&kinetic(), "k.csv").unwrap();
        assert!(r.doubts.is_empty());
        assert_eq!(r.blocks.len(), 1);
    }

    #[test]
    fn a_comma_that_may_be_a_decimal_is_flagged() {
        let mut s = String::from("a,b,c,d\n");
        for k in 0..6 {
            s += &format!("89{k},69, 2397,76{k}\n");
        }
        let r = sniff(s.as_bytes(), "f").unwrap();
        assert_eq!(r.recipe.separator, Separator::Comma);
        assert_eq!(r.doubts, vec![Doubt::DecimalOrSeparator { line: 2 }]);
        let plain = sniff(b"a,b\n1.5,2.5\n3.5,4.5\n5.5,6.5\n", "f").unwrap();
        assert!(plain.doubts.is_empty());
        // One ambiguous row in ten is not enough.
        let mut few = String::from("a,b,c\n");
        for _ in 0..9 {
            few += "1.5,2.5,3.5\n";
        }
        few += "1,5, 2\n";
        assert!(sniff(few.as_bytes(), "f").unwrap().doubts.is_empty());
    }

    #[test]
    fn the_block_search_is_bounded() {
        let bytes = "1,2\n3,4\nx\n".repeat(100_000);
        let started = std::time::Instant::now();
        let r = sniff(bytes.as_bytes(), "f").unwrap();
        assert!(r.blocks.len() <= 1001, "{}", r.blocks.len());
        assert!(r.blocks.len() > 1);
        // The titles cannot all be read as one table when the search was cut.
        assert!(matches!(
            r.doubts[0],
            Doubt::SeveralBlocks { split: false, .. }
        ));
        assert!(!r.recipe.skip_text_rows);
        assert!(started.elapsed().as_secs() < 20);
    }

    #[test]
    fn a_one_cell_title_line_is_not_a_long_name_row() {
        let bytes =
            b"Spectrum data,,,,\n[nm],A1,B1,C1,D1\n400,0.1,0.2,0.3,0.4\n401,0.1,0.2,0.3,0.4\n";
        let r = sniff(bytes, "f").unwrap();
        assert!(r.recipe.has_column_header_row);
        assert_eq!(r.recipe.subheader_roles.long_name, None);
        assert_eq!(r.recipe.subheader_roles.unit, None);
        assert!(!r.recipe.subheader_roles.same_line_name_unit);
        let t = crate::parse(bytes, &r.recipe).unwrap();
        assert_eq!(t.columns[1].name.as_deref(), Some("A1"));
        assert_eq!(t.columns[1].unit, None);
    }

    // -- instruments, names of known layouts ---------------------------

    fn ocean(extra_header: usize, names: bool, tail: &str) -> Vec<u8> {
        let mut t = String::from("SpectraSuite Data File\n");
        t += &format!("{}\n", "+".repeat(36));
        t += "Date: Mon Jan 14 17:36:35 GMT 2008\nSpectrometers: USB4A00428\n";
        for k in 0..extra_header {
            t += &format!("Extra {k}: yes\n");
        }
        t += ">>>>>Begin Processed Spectral Data<<<<<\n";
        if names {
            t += "W\tD\tS\tP\n";
        }
        for k in 0..8 {
            let wl = 200.0 + f64::from(k);
            t += &if names {
                format!("{wl:.2}\t1.5\t2.5\t{k}.25\n")
            } else {
                format!("{wl:.2}\t{k}.25\n")
            };
        }
        t += tail;
        t.into_bytes()
    }

    #[test]
    fn an_ocean_marker_fits_headers_of_any_length() {
        let end = ">>>>>End Processed Spectral Data<<<<<\n";
        let short = ocean(0, false, end);
        let long = ocean(7, false, end);
        let a = sniff(&short, "a.txt").unwrap();
        let b = sniff(&long, "b.txt").unwrap();
        for s in [&a, &b] {
            assert_eq!(
                s.instrument.as_ref().map(|m| m.instrument),
                Some(Instrument::OceanOptics)
            );
            assert_eq!(
                s.recipe.main_until_marker.as_deref(),
                Some(">>>>>Begin Processed Spectral Data<<<<<")
            );
            assert_eq!(
                s.recipe.data_end,
                DataEnd::Marker(">>>>>End Processed Spectral Data<<<<<".into())
            );
            assert_eq!(s.rows, 8);
        }
        // The recipe of the short file reads the long one.
        let t = crate::parse(&long, &a.recipe).unwrap();
        assert_eq!(t.row_count(), 8);
        assert_eq!(t.columns.len(), 2);
    }

    #[test]
    fn the_ocean_names_row_stays_and_a_calibration_block_is_not_data() {
        let tail = ">>>>>End Processed Spectral Data<<<<<\n>>>>>Begin Calibration Data<<<<<\n[uJoule/count]\n1.5\n2.5\n3.5\n>>>>>End Calibration Data<<<<<\n";
        let bytes = ocean(0, true, tail);
        let s = sniff(&bytes, "x.JazIrrad").unwrap();
        assert!(s.recipe.has_column_header_row);
        let t = crate::parse(&bytes, &s.recipe).unwrap();
        assert_eq!(t.row_count(), 8);
        let names: Vec<_> = t.columns.iter().map(|c| c.name.clone().unwrap()).collect();
        assert_eq!(names, ["W", "D", "S", "P"]);
    }

    #[test]
    fn an_ocean_data_block_without_end_marker_stops_at_the_next_block() {
        let tail = ">>>>>Begin Calibration Data<<<<<\n[uJoule/count]\n1.5\n2.5\n3.5\n";
        let bytes = ocean(0, false, tail);
        let s = sniff(&bytes, "x.JazIrrad").unwrap();
        assert_eq!(
            s.recipe.data_end,
            DataEnd::Marker(">>>>>Begin Calibration Data<<<<<".into())
        );
        assert_eq!(crate::parse(&bytes, &s.recipe).unwrap().row_count(), 8);
        // No block after it: the data run to the end of the file.
        let bare = ocean(0, false, "");
        let s = sniff(&bare, "x.txt").unwrap();
        assert_eq!(crate::parse(&bare, &s.recipe).unwrap().row_count(), 8);
    }

    #[test]
    fn avantes_units_lose_their_brackets_across_a_blank_line() {
        let mut t = String::from(
            "\nIntegration time [ms]:    6.00\nAveraging Nr. [scans]: 79\nSmoothing Nr. [pixels]: 1\nData measured with spectrometer [name]: 1209167U1\nWave   ;Sample   ;Dark     ;Reflectance\n[nm]   ;[counts] ;[counts] ;[%]\n\n",
        );
        for k in 0..6 {
            t += &format!(" {}.00;   -6.8{k};  -10.33;252.16\n", 300 + k);
        }
        let s = sniff(t.as_bytes(), "avasoft8.txt").unwrap();
        assert_eq!(
            s.instrument.as_ref().map(|m| (m.instrument, m.line)),
            Some((Instrument::Avantes, 2))
        );
        let table = crate::parse(t.as_bytes(), &s.recipe).unwrap();
        assert_eq!(table.row_count(), 6);
        let names: Vec<_> = table
            .columns
            .iter()
            .map(|c| (c.long_name.clone().unwrap(), c.unit.clone().unwrap()))
            .collect();
        assert_eq!(
            names,
            [
                ("Wave".to_string(), "nm".to_string()),
                ("Sample".to_string(), "counts".to_string()),
                ("Dark".to_string(), "counts".to_string()),
                ("Reflectance".to_string(), "%".to_string()),
            ]
        );
    }

    fn netzsch(names: &str) -> Vec<u8> {
        let mut t =
            String::from("#FORMAT:NETZSCH5\n#DECIMAL:POINT\n#SEPARATOR:SEMICOLON\n#SAMPLE:x\n\n");
        t += names;
        t += "\n";
        for k in 0..6 {
            t += &format!("{}.5;{k}.1;{k}.25\n", 25 + k);
        }
        t.into_bytes()
    }

    #[test]
    fn netzsch_names_split_at_the_first_slash() {
        let bytes = netzsch("##Temp./\u{b0}C;Time/min;DSC/(mW/mg)");
        let s = sniff(&bytes, "n.txt").unwrap();
        assert_eq!(
            s.instrument.as_ref().map(|m| m.instrument),
            Some(Instrument::Netzsch)
        );
        assert!(s.recipe.name_unit_slash);
        let t = crate::parse(&bytes, &s.recipe).unwrap();
        let got: Vec<_> = t
            .columns
            .iter()
            .map(|c| (c.long_name.clone().unwrap(), c.unit.clone().unwrap()))
            .collect();
        assert_eq!(
            got,
            [
                ("Temp.".to_string(), "\u{b0}C".to_string()),
                ("Time".to_string(), "min".to_string()),
                ("DSC".to_string(), "mW/mg".to_string()),
            ]
        );
        // No bracket in the line, still a names line with units.
        assert_eq!(t.row_count(), 6);
    }

    #[test]
    fn a_slash_in_the_names_of_another_file_is_not_split() {
        let s = sniff(b"counts/s;t\n1;2\n3;4\n5;6\n", "x.csv").unwrap();
        assert!(!s.recipe.name_unit_slash);
        let t = crate::parse(b"counts/s;t\n1;2\n3;4\n5;6\n", &s.recipe).unwrap();
        assert_eq!(t.columns[0].name.as_deref(), Some("counts/s"));
    }

    fn plate(with_times: bool) -> Vec<u8> {
        let mut t = String::from("read_a,,,,,,\nread_b,,,,,,\n");
        for k in 0..3 {
            if with_times {
                t += &format!("{}s,,,,,,\n", k * 21);
            } else {
                t += &format!("label {k},,,,,,\n");
            }
            t += &format!("{},2,3,4,5,6,7\n", 100 + k);
            t += &format!("Overflow,Overflow,Overflow,Overflow,Overflow,{k},9\n");
            t += &format!("Overflow,Overflow,Overflow,Overflow,Overflow,{k},8\n");
            t += ",,,,,,\n,,,,,,\n";
        }
        t.into_bytes()
    }

    #[test]
    fn a_plate_reader_keeps_its_mostly_overflow_rows_as_data() {
        let s = sniff(&plate(true), "p.csv").unwrap();
        assert_eq!(
            s.instrument.as_ref().map(|m| m.instrument),
            Some(Instrument::PlateReader)
        );
        assert_eq!(s.blocks.len(), 3);
        assert!(s.blocks.iter().all(|b| b.rows == 3), "{:?}", s.blocks);
        // The same rows with no plate signature: the half-numbers rule drops them.
        let plain = sniff(&plate(false), "p.csv").unwrap();
        assert_eq!(plain.instrument, None);
        assert!(plain.blocks.iter().any(|b| b.rows < 3));
    }

    #[test]
    fn the_instrument_of_the_tuned_files_follows_their_family() {
        for (name, expected) in [
            ("jasco_fluo_tab.txt", Some(Instrument::Jasco)),
            ("JascoUV_vis.csv", Some(Instrument::Jasco)),
            ("ir_jcamp_lookalike.txt", Some(Instrument::Jcamp)),
            ("generic_named_header.csv", None),
            ("rpe_nohead_tab.asc", None),
            ("analytikjena_nohead_semicolon.csv", None),
        ] {
            let r = sniff(&public(name), name).unwrap();
            assert_eq!(r.instrument.map(|m| m.instrument), expected, "{name}");
            assert_eq!(r.recipe.thousands, None, "{name}");
        }
        let r = sniff(&kinetic(), "k.csv").unwrap();
        assert_eq!(r.family, Some(Family::Jasco));
        assert_eq!(r.instrument.map(|m| m.instrument), Some(Instrument::Jasco));
        assert_eq!(r.thousands, None);
    }

    #[test]
    fn a_jcamp_from_ocean_optics_names_its_vendor() {
        let text = "##TITLE=x\n##JCAMP-DX=5\n##ORIGIN= OCEANOPTICS EXPORT\n##FIRSTX=0\n##LASTX=1\n##XYDATA=(X++(Y..Y))\n0 1 2\n##END=\n";
        let s = sniff(text.as_bytes(), "o.jdx").unwrap();
        assert_eq!(
            s.instrument.as_ref().map(|m| m.instrument),
            Some(Instrument::JcampOcean)
        );
        let plain = text.replace("OCEANOPTICS EXPORT", "SOMEONE");
        let s = sniff(plain.as_bytes(), "o.jdx").unwrap();
        assert_eq!(
            s.instrument.as_ref().map(|m| m.instrument),
            Some(Instrument::Jcamp)
        );
    }

    #[test]
    fn a_plain_table_with_a_date_header_has_no_instrument() {
        let s = sniff(b"Date: 2020-01-01\nUser: me\nt;v\n1;2\n3;4\n5;6\n", "x.csv").unwrap();
        assert_eq!(s.instrument, None);
    }

    const IR_BLOCK: &str = "##TITLE=ir\n##JCAMP-DX=4.24\n##DATA TYPE=INFRARED SPECTRUM\n##XUNITS=1/CM\n##YUNITS=ABSORBANCE\n##FIRSTX=400\n##LASTX=403\n##NPOINTS=4\n##XFACTOR=1\n##YFACTOR=1\n##XYDATA=(X++(Y..Y))\n400 1 2 3 4\n##END=\n";

    #[test]
    fn nmr_data_is_refused_in_every_form() {
        let spectrum = "##TITLE=s\n##JCAMP-DX=5\n##DATA TYPE=NMR SPECTRUM\n##FIRSTX=0\n##LASTX=3\n##NPOINTS=4\n##XYDATA=(X++(Y..Y))\n0 1 2 3 4\n##END=\n";
        let fid = "##TITLE=f\n##JCAMP-DX=5\n##DATA TYPE=NMR FID\n##NTUPLES=NMR FID\n##VAR_NAME=TIME, REAL\n##SYMBOL=X, R\n##VAR_DIM=4, 4\n##PAGE=N=1\n##DATA TABLE=(X++(R..R)), XYDATA\n0 1 2 3 4\n##END NTUPLES=NMR FID\n##END=\n";
        let two_d = "##TITLE=n\n##JCAMP-DX=5\n##DATA TYPE=nD NMR SPECTRUM\n##NTUPLES=nD NMR SPECTRUM\n##PAGE=F1=1\n##DATA TABLE=(F2++(Y..Y)), PROFILE\n0 1 2 3\n##END=\n";
        let link = "##TITLE=l\n##JCAMP-DX=5\n##DATA TYPE=LINK\n##BLOCKS=1\n##TITLE=a\n##DATA TYPE=NMR PEAK ASSIGNMENTS\n##PEAK ASSIGNMENTS=(XYA)\n(1.0,2.0,<a>)\n##END=\n##END=\n";
        let observe =
            "##TITLE=o\n##.OBSERVE FREQUENCY=400.1\n##XYDATA=(X++(Y..Y))\n0 1 2 3 4\n##END=\n";
        let ntuples_only = two_d.replace("##DATA TYPE=nD NMR SPECTRUM\n", "");
        for (name, text) in [
            ("ntuples only", ntuples_only.as_str()),
            ("spectrum", spectrum),
            ("fid", fid),
            ("2d", two_d),
            ("link", link),
            ("observe", observe),
        ] {
            assert_eq!(
                sniff(text.as_bytes(), "n.jdx").unwrap_err(),
                SniffRefusal::Nmr,
                "{name}"
            );
        }
    }

    #[test]
    fn non_nmr_jcamp_is_not_refused() {
        assert!(sniff(IR_BLOCK.as_bytes(), "i.jdx").is_ok());
        let ms = "##TITLE=m\n##JCAMP-DX=5\n##DATA TYPE=MASS SPECTRUM\n##NTUPLES=MASS SPECTRUM\n##VAR_NAME=MASS, ABUNDANCE\n##SYMBOL=X, Y\n##VAR_DIM=3, 3\n##PAGE=N=1\n##DATA TABLE=(XY..XY), PEAKS\n1,2 3,4 5,6\n##END NTUPLES=MASS SPECTRUM\n##END=\n";
        assert!(sniff(ms.as_bytes(), "m.jdx").is_ok());
    }

    #[test]
    fn a_file_with_ir_and_nmr_blocks_keeps_the_ir_table_only() {
        let nmr = "##TITLE=nmr\n##JCAMP-DX=4.24\n##DATA TYPE=NMR SPECTRUM\n##FIRSTX=0\n##LASTX=3\n##XYDATA=(X++(Y..Y))\n0 9 9 9 9\n##END=\n";
        let text = format!("{IR_BLOCK}{nmr}");
        let s = sniff(text.as_bytes(), "m.jdx").unwrap();
        assert_eq!(s.jcamp.as_ref().map(|j| j.blocks), Some(1));
        assert!(s.doubts.iter().any(|d| matches!(
            d,
            Doubt::Jcamp {
                issue: crate::jcamp::JcampIssue::NmrLeftOut { blocks: 1 },
                ..
            }
        )));
        let mut recipe = s.recipe.clone();
        recipe.format = crate::recipe::FileFormat::Jcamp;
        let tables = crate::parse::parse_blocks(text.as_bytes(), &recipe).unwrap();
        assert_eq!(tables.len(), 1);
        assert_eq!(tables[0].columns[1].values, vec![1.0, 2.0, 3.0, 4.0]);
    }

    #[test]
    fn parsing_an_nmr_only_file_as_jcamp_reads_nothing() {
        let nmr = "##TITLE=nmr\n##DATA TYPE=NMR SPECTRUM\n##FIRSTX=0\n##LASTX=3\n##XYDATA=(X++(Y..Y))\n0 9 9 9 9\n##END=\n";
        let mut recipe = ImportRecipe::new("x");
        recipe.format = crate::recipe::FileFormat::Jcamp;
        assert!(crate::parse::parse_blocks(nmr.as_bytes(), &recipe).is_err());
    }

    /// A small JCAMP-DX file with a compressed table, a second block and a
    /// footer, to cut at every byte.
    fn jcamp_sample() -> Vec<u8> {
        "##TITLE=a\n##JCAMP-DX=4.24\n##DATA TYPE=INFRARED SPECTRUM\n##XFACTOR=1\n\
         ##YFACTOR=0.5\n##FIRSTX=400\n##LASTX=406\n##NPOINTS=7\n\
         ##XYDATA=(X++(Y..Y))\n400 a1j2K3\n403 @5%3\n##END=\n\
         ##TITLE=b\n##XYPOINTS=(XY..XY)\n1,5;2 2,5;3\n##END=\n"
            .as_bytes()
            .to_vec()
    }

    /// Invariant 10: whatever prefix of a file the detector and the import
    /// are given, they return a value and never panic.
    #[test]
    fn every_prefix_of_a_jcamp_and_of_a_kinetic_file_is_handled() {
        for full in [jcamp_sample(), kinetic()] {
            let whole = sniff(&full, "f").unwrap().recipe;
            for end in 0..=full.len() {
                let bytes = &full[..end];
                if let Ok(found) = sniff(bytes, "f") {
                    let _ = crate::parse::parse_blocks(bytes, &found.recipe);
                }
                let _ = crate::parse::parse_blocks(bytes, &whole);
                let _ = crate::preview::preview(bytes, &whole);
            }
        }
    }
}

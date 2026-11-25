//! Reading and editing the TOML text of a Script Settings object (#548).
//!
//! **Why this module exists.** The model stores a Script Settings as raw
//! text (`NodeKind::ScriptSettings`) and never parses it. The form of the
//! window (lot L3) and the `pyst` calls (lot L4) both need the same two
//! things: a structured reading of that text, and a way to change one value
//! **without touching any other byte**: comments, order, spacing and
//! end-of-line comments are the user's, so a rewrite through a serializer
//! (which would drop them) is not an option. `toml_edit` keeps the layout;
//! this module is the only place that knows it, so core stays free of UI and
//! the two callers cannot disagree on what a value, a help line or a hint is.
//! Edits change only the edited value in an LF text. A CRLF text would come
//! back mixed (`toml_edit` writes LF everywhere except inside multi-line
//! strings, whose source bytes it keeps), so the commands store LF
//! ([`normalize_newlines`], #571).
//!
//! **What it is not.** It does not repair or guess: a text that is not exact
//! TOML is refused with the parser's message and its 1-based line
//! ([`ParseError`]). It never mutates the model, never runs in the session
//! and holds no state: every edit takes a parsed [`SettingsDoc`] and returns
//! the **new full text**, which the caller turns into a `SetScriptSettingsText`
//! command.
//!
//! **Number reading is not here.** The form's number rules (S-N4 comma
//! accepted, S-N6 refused, S-N7 clamped) live with the egui-free number
//! building block of the application crate (`Verdict` of `field_state.rs`),
//! which already has the vocabulary. Core only offers the typed value and the
//! bounds ([`Hint::min`], [`Hint::max`]) that the form feeds it.
//!
//! **Bounded input (invariant 10).** A text over [`MAX_TEXT_BYTES`] is refused
//! before parsing; nesting depth is bounded by the parser itself (checked by
//! the pathological-input test), so the recursive walks below are bounded too.
//!
//! # Hints
//!
//! The comment lines directly above a key (no blank line between) are its
//! help. A line that is, as a whole and ignoring case, one of these is a hint
//! and is removed from the help text:
//!
//! - `choices: a | b | c` (at least two non-empty items): a choice list;
//! - `between A and B` (numbers, `A <= B`, inclusive): minimum and maximum;
//! - `at least A`: minimum; `at most B`: maximum.
//!
//! A line that does not parse fully (`between x and 3`) stays plain help.
//! Hints never refuse a write ([`apply_values`]); [`Entry::out_of_hint`]
//! only reports.

use std::borrow::Cow;
use std::str::FromStr;

use toml_edit::{Array, ArrayOfTables, DocumentMut, Item, Table, TableLike, Value};

/// Longest text accepted, in bytes. A settings file is a few kilobytes; the
/// cap keeps a pasted binary or a pathological file from reaching the parser.
pub const MAX_TEXT_BYTES: usize = 4 * 1024 * 1024;

/// The text with its CRLF line endings turned into LF (#571). Commands store
/// this form, so the text of an object written by a command never depends on
/// the platform that produced it.
///
/// A CR is dropped only when it is followed by LF **and** not preceded by
/// another CR: `\r\r\n` (refused by TOML) must stay refused, and a second
/// pass must change nothing. A lone CR is untouched. Text without a CRLF is
/// borrowed, not copied.
pub fn normalize_newlines(text: &str) -> Cow<'_, str> {
    if !text.contains("\r\n") {
        return Cow::Borrowed(text);
    }
    let bytes = text.as_bytes();
    let mut out = String::with_capacity(text.len());
    let mut start = 0;
    for (i, &b) in bytes.iter().enumerate() {
        if b == b'\r' && bytes.get(i + 1) == Some(&b'\n') && (i == 0 || bytes[i - 1] != b'\r') {
            out.push_str(&text[start..i]);
            start = i + 1;
        }
    }
    out.push_str(&text[start..]);
    Cow::Owned(out)
}

/// True when both texts are the same, line endings aside (see
/// [`normalize_newlines`]). The "nothing to write" test of the form and of
/// `pyst`: an object kept in CRLF equals its LF twin.
pub fn same_text(a: &str, b: &str) -> bool {
    a == b || normalize_newlines(a) == normalize_newlines(b)
}

/// One step of the path to a key: a table key or the 0-based index of a
/// block of an array of tables (`[[x]]`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Seg {
    /// A key name.
    Key(String),
    /// The 0-based index of an array-of-tables block.
    Index(usize),
}

fn path_text(path: &[Seg]) -> String {
    let mut out = String::new();
    for seg in path {
        match seg {
            Seg::Key(key) => {
                if !out.is_empty() {
                    out.push('.');
                }
                out.push_str(key);
            }
            Seg::Index(i) => out.push_str(&format!("[{}]", i + 1)),
        }
    }
    out
}

/// The kind of a scalar value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScalarKind {
    /// Text.
    String,
    /// 64-bit integer.
    Integer,
    /// 64-bit float.
    Float,
    /// `true` / `false`.
    Bool,
    /// Date and time with an offset.
    OffsetDateTime,
    /// Date and time without offset.
    LocalDateTime,
    /// Date only.
    LocalDate,
    /// Time only.
    LocalTime,
}

impl ScalarKind {
    fn name(self) -> &'static str {
        match self {
            ScalarKind::String => "string",
            ScalarKind::Integer => "integer",
            ScalarKind::Float => "float",
            ScalarKind::Bool => "boolean",
            ScalarKind::OffsetDateTime => "offset date-time",
            ScalarKind::LocalDateTime => "local date-time",
            ScalarKind::LocalDate => "local date",
            ScalarKind::LocalTime => "local time",
        }
    }
}

/// A scalar TOML value. Integers are kept exactly; dates keep their kind
/// (a date-only value stays date-only) and are carried as their TOML text.
#[derive(Clone, Debug, PartialEq)]
pub enum Scalar {
    /// Text.
    String(String),
    /// Exact integer.
    Integer(i64),
    /// Float.
    Float(f64),
    /// Boolean.
    Bool(bool),
    /// `1979-05-27T07:32:00Z`.
    OffsetDateTime(String),
    /// `1979-05-27T07:32:00`.
    LocalDateTime(String),
    /// `1979-05-27`.
    LocalDate(String),
    /// `07:32:00`.
    LocalTime(String),
}

impl Scalar {
    /// The kind of this value.
    pub fn kind(&self) -> ScalarKind {
        match self {
            Scalar::String(_) => ScalarKind::String,
            Scalar::Integer(_) => ScalarKind::Integer,
            Scalar::Float(_) => ScalarKind::Float,
            Scalar::Bool(_) => ScalarKind::Bool,
            Scalar::OffsetDateTime(_) => ScalarKind::OffsetDateTime,
            Scalar::LocalDateTime(_) => ScalarKind::LocalDateTime,
            Scalar::LocalDate(_) => ScalarKind::LocalDate,
            Scalar::LocalTime(_) => ScalarKind::LocalTime,
        }
    }

    fn from_value(value: &Value) -> Option<Scalar> {
        match value {
            Value::String(s) => Some(Scalar::String(s.value().clone())),
            Value::Integer(i) => Some(Scalar::Integer(*i.value())),
            Value::Float(f) => Some(Scalar::Float(*f.value())),
            Value::Boolean(b) => Some(Scalar::Bool(*b.value())),
            Value::Datetime(d) => {
                let dt = d.value();
                let text = dt.to_string();
                Some(
                    match (dt.date.is_some(), dt.time.is_some(), dt.offset.is_some()) {
                        (true, true, true) => Scalar::OffsetDateTime(text),
                        (true, true, false) => Scalar::LocalDateTime(text),
                        (true, false, _) => Scalar::LocalDate(text),
                        _ => Scalar::LocalTime(text),
                    },
                )
            }
            Value::Array(_) | Value::InlineTable(_) => None,
        }
    }

    /// A text read as a date or time of `kind` (the form's date field): `None`
    /// for another kind, or for a text that is not a valid date of that kind
    /// (a time where a date is stored, `2026-13-40`). The text is trimmed.
    pub fn date_of_kind(kind: ScalarKind, text: &str) -> Option<Scalar> {
        let text = text.trim().to_string();
        let scalar = match kind {
            ScalarKind::OffsetDateTime => Scalar::OffsetDateTime(text),
            ScalarKind::LocalDateTime => Scalar::LocalDateTime(text),
            ScalarKind::LocalDate => Scalar::LocalDate(text),
            ScalarKind::LocalTime => Scalar::LocalTime(text),
            _ => return None,
        };
        scalar.to_value()?;
        Some(scalar)
    }

    /// The TOML value for this scalar; a date that does not read back as its
    /// own kind is `None`.
    fn to_value(&self) -> Option<Value> {
        Some(match self {
            Scalar::String(s) => Value::from(s.as_str()),
            Scalar::Integer(i) => Value::from(*i),
            Scalar::Float(f) => Value::from(*f),
            Scalar::Bool(b) => Value::from(*b),
            Scalar::OffsetDateTime(t)
            | Scalar::LocalDateTime(t)
            | Scalar::LocalDate(t)
            | Scalar::LocalTime(t) => {
                let value = Value::from(toml_edit::Datetime::from_str(t).ok()?);
                // A date that reads back as another kind (a time for a date) is refused.
                if Scalar::from_value(&value)?.kind() != self.kind() {
                    return None;
                }
                value
            }
        })
    }
}

/// What the elements of an array look like.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ElementKind {
    /// No element: nothing to copy the type from.
    Empty,
    /// Every element is a scalar of this kind.
    Uniform(ScalarKind),
    /// Several kinds, nested arrays or tables: the form shows it read-only.
    Mixed,
}

/// The value of a key.
#[derive(Clone, Debug, PartialEq)]
pub enum EntryValue {
    /// A scalar.
    Scalar(Scalar),
    /// An array of scalars of one kind (or empty).
    Array {
        /// The kind of the elements.
        kind: ElementKind,
        /// The elements, in order.
        items: Vec<Scalar>,
    },
    /// An array that is not a plain list of one scalar kind: read-only.
    MixedArray,
}

/// What the comment lines above a key ask of its value.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Hint {
    /// `choices: a | b | c`.
    pub choices: Option<Vec<String>>,
    /// Inclusive lower bound.
    pub min: Option<f64>,
    /// Inclusive upper bound.
    pub max: Option<f64>,
}

impl Hint {
    /// Whether `value` is outside this hint's choices or bounds (one element of
    /// a list, for the form's per-field orange frame).
    pub fn rejects(&self, value: &Scalar) -> bool {
        scalar_out_of_hint(value, self)
    }
}

/// One key of a section.
#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    /// The key name (last segment).
    pub key: String,
    /// The full path, to hand back to the edit functions.
    pub path: Vec<Seg>,
    /// The stored value.
    pub value: EntryValue,
    /// The comment lines directly above the key, hints removed, joined by `\n`.
    pub help: String,
    /// The hints read from those lines.
    pub hint: Hint,
}

impl Entry {
    /// Whether the stored value is outside its choices or bounds. The form
    /// shows it orange; nothing ever rewrites it.
    pub fn out_of_hint(&self) -> bool {
        match &self.value {
            EntryValue::Scalar(s) => scalar_out_of_hint(s, &self.hint),
            EntryValue::Array { items, .. } => {
                items.iter().any(|s| scalar_out_of_hint(s, &self.hint))
            }
            EntryValue::MixedArray => false,
        }
    }
}

fn scalar_out_of_hint(value: &Scalar, hint: &Hint) -> bool {
    if let Some(choices) = &hint.choices {
        let listed = match value {
            Scalar::String(s) => choices.iter().any(|c| c == s),
            Scalar::Integer(i) => choices.iter().any(|c| c.parse::<i64>().ok() == Some(*i)),
            Scalar::Float(f) => choices.iter().any(|c| c.parse::<f64>().ok() == Some(*f)),
            Scalar::Bool(b) => choices.iter().any(|c| c == &b.to_string()),
            _ => true,
        };
        if !listed {
            return true;
        }
    }
    let number = match value {
        Scalar::Integer(i) => Some(*i as f64),
        Scalar::Float(f) => Some(*f),
        _ => None,
    };
    if let Some(n) = number {
        if hint.min.is_some_and(|m| n < m) || hint.max.is_some_and(|m| n > m) {
            return true;
        }
    }
    false
}

/// A titled group of keys: the root keys, one `[table]`, one inline or dotted
/// table, or one `[[array of tables]]` block.
#[derive(Clone, Debug, PartialEq)]
pub struct Section {
    /// Path of the table (empty for the root keys).
    pub path: Vec<Seg>,
    /// 1-based index when the section is a `[[x]]` block.
    pub block: Option<usize>,
    /// The keys, in file order.
    pub entries: Vec<Entry>,
}

impl Section {
    /// The key names of the path, without block indexes: the title segments.
    pub fn names(&self) -> Vec<&str> {
        self.path
            .iter()
            .filter_map(|s| match s {
                Seg::Key(k) => Some(k.as_str()),
                Seg::Index(_) => None,
            })
            .collect()
    }
}

/// A neutral ordered tree for the Python bridge.
#[derive(Clone, Debug, PartialEq)]
pub enum SettingsValue {
    /// A scalar.
    Scalar(Scalar),
    /// An array (a `[[x]]` array of tables is a list of maps).
    List(Vec<SettingsValue>),
    /// A table, keys in file order.
    Map(Vec<(String, SettingsValue)>),
}

/// A text that is not exact TOML.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("line {line}, column {column}: {message}")]
pub struct ParseError {
    /// 1-based line.
    pub line: usize,
    /// 1-based column (in characters).
    pub column: usize,
    /// The parser's message.
    pub message: String,
}

/// Why an edit was refused. Nothing is written in any case.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum EditError {
    /// The text itself is not TOML.
    #[error("{0}")]
    Parse(ParseError),
    /// The text is larger than [`MAX_TEXT_BYTES`].
    #[error("text too large ({0} bytes)")]
    TooLarge(usize),
    /// No such key.
    #[error("no such key: {0}")]
    NotFound(String),
    /// The new value does not have the type of the stored one.
    #[error("{key}: expected {expected}, got {found}")]
    TypeMismatch {
        /// Dotted path of the key.
        key: String,
        /// Type of the stored value.
        expected: &'static str,
        /// Type of the new value.
        found: &'static str,
    },
    /// The key is not an array of one scalar kind.
    #[error("{0}: not an array of one kind of value")]
    NotAnArray(String),
    /// Element index out of range.
    #[error("{0}: no such element")]
    NoSuchElement(String),
    /// An empty array has no element to copy.
    #[error("{0}: the array is empty, add the first value in the text")]
    EmptyArray(String),
    /// A date value that does not read as a date of its kind.
    #[error("{0}: not a valid date or time")]
    BadDate(String),
    /// A value that cannot be stored (for example a list mixing kinds).
    #[error("{0}: unsupported value")]
    Unsupported(String),
}

/// A parsed Script Settings text.
#[derive(Clone, Debug)]
pub struct SettingsDoc {
    doc: DocumentMut,
    source: String,
    /// The sections, in file order. Empty for an empty document.
    pub sections: Vec<Section>,
}

/// Parses `text`. Malformed TOML is refused, never repaired.
pub fn parse(text: &str) -> Result<SettingsDoc, ParseError> {
    parse_inner(text).map_err(|e| match e {
        EditError::Parse(p) => p,
        other => ParseError {
            line: 1,
            column: 1,
            message: other.to_string(),
        },
    })
}

fn parse_inner(text: &str) -> Result<SettingsDoc, EditError> {
    if text.len() > MAX_TEXT_BYTES {
        return Err(EditError::TooLarge(text.len()));
    }
    let doc = text.parse::<DocumentMut>().map_err(|e| {
        let offset = e.span().map_or(0, |s| s.start).min(text.len());
        let (line, column) = line_col(text, offset);
        // The parser leaves the message empty when the text stops too early.
        let message = if e.message().is_empty() {
            "unexpected end of text, a value is missing".to_string()
        } else {
            e.message().to_string()
        };
        EditError::Parse(ParseError {
            line,
            column,
            message,
        })
    })?;
    let mut sections = Vec::new();
    collect_sections(doc.as_table(), 0, false, &mut Vec::new(), &mut sections);
    sections.sort_by_key(|(order, _)| *order);
    Ok(SettingsDoc {
        doc,
        source: text.to_string(),
        sections: sections.into_iter().map(|(_, s)| s).collect(),
    })
}

fn line_col(text: &str, offset: usize) -> (usize, usize) {
    let mut offset = offset;
    while !text.is_char_boundary(offset) {
        offset -= 1;
    }
    let before = &text[..offset];
    let line = before.matches('\n').count() + 1;
    let column = before.rsplit('\n').next().map_or(0, |l| l.chars().count()) + 1;
    (line, column)
}

type Ordered = Vec<(usize, Section)>;

fn collect_sections(
    table: &dyn TableLike,
    order: usize,
    explicit: bool,
    path: &mut Vec<Seg>,
    out: &mut Ordered,
) {
    let slot = out.len();
    let mut entries = Vec::new();
    let mut children: Vec<(String, &Item)> = Vec::new();
    for (key, item) in table.iter() {
        match item {
            Item::Value(Value::InlineTable(_)) | Item::Table(_) | Item::ArrayOfTables(_) => {
                children.push((key.to_string(), item));
            }
            Item::Value(value) => {
                if let Some(entry) = read_entry(table, key, value, path) {
                    entries.push(entry);
                }
            }
            Item::None => {}
        }
    }
    if !entries.is_empty() || explicit {
        let block = match path.last() {
            Some(Seg::Index(i)) => Some(i + 1),
            _ => None,
        };
        out.insert(
            slot,
            (
                order,
                Section {
                    path: path.clone(),
                    block,
                    entries,
                },
            ),
        );
    }
    for (key, item) in children {
        path.push(Seg::Key(key));
        match item {
            Item::Table(t) => {
                let at = t.position().unwrap_or(order);
                collect_sections(t, at, !t.is_implicit() && !t.is_dotted(), path, out);
            }
            Item::Value(Value::InlineTable(t)) => collect_sections(t, order, false, path, out),
            Item::ArrayOfTables(blocks) => {
                for (i, block) in blocks.iter().enumerate() {
                    path.push(Seg::Index(i));
                    collect_sections(block, block.position().unwrap_or(order), true, path, out);
                    path.pop();
                }
            }
            _ => {}
        }
        path.pop();
    }
}

fn read_entry(table: &dyn TableLike, key: &str, value: &Value, path: &[Seg]) -> Option<Entry> {
    let entry_value = match value {
        Value::Array(array) => read_array(array),
        other => EntryValue::Scalar(Scalar::from_value(other)?),
    };
    let prefix = table
        .get_key_value(key)
        .and_then(|(k, _)| k.leaf_decor().prefix())
        .and_then(|p| p.as_str())
        .unwrap_or("");
    let (help, hint) = read_help(prefix);
    let mut full = path.to_vec();
    full.push(Seg::Key(key.to_string()));
    Some(Entry {
        key: key.to_string(),
        path: full,
        value: entry_value,
        help,
        hint,
    })
}

fn read_array(array: &Array) -> EntryValue {
    let mut kind = ElementKind::Empty;
    let mut items = Vec::new();
    for value in array.iter() {
        let Some(scalar) = Scalar::from_value(value) else {
            return EntryValue::MixedArray;
        };
        match kind {
            ElementKind::Empty => kind = ElementKind::Uniform(scalar.kind()),
            ElementKind::Uniform(k) if k == scalar.kind() => {}
            _ => return EntryValue::MixedArray,
        }
        items.push(scalar);
    }
    EntryValue::Array { kind, items }
}

/// Help text and hints from the decor prefix of a key: the contiguous block
/// of `#` lines directly above it.
fn read_help(prefix: &str) -> (String, Hint) {
    let mut lines: Vec<&str> = prefix.split('\n').collect();
    // The last piece is the indentation before the key, not a line.
    lines.pop();
    let mut block = Vec::new();
    for line in lines.iter().rev() {
        let t = line.trim();
        match t.strip_prefix('#') {
            Some(rest) => block.push(rest.strip_prefix(' ').unwrap_or(rest).trim_end()),
            None => break,
        }
    }
    block.reverse();
    let mut hint = Hint::default();
    let mut help = Vec::new();
    for line in block {
        if !apply_hint_line(line, &mut hint) {
            help.push(line);
        }
    }
    (help.join("\n"), hint)
}

fn apply_hint_line(line: &str, hint: &mut Hint) -> bool {
    let lower = line.trim().to_lowercase();
    if let Some(rest) = lower.strip_prefix("choices:") {
        // Case of the items is kept: re-read from the original line.
        let original = line.trim();
        let items: Vec<String> = original[original.len() - rest.len()..]
            .split('|')
            .map(|s| s.trim().to_string())
            .collect();
        if items.len() >= 2 && items.iter().all(|s| !s.is_empty()) {
            hint.choices = Some(items);
            return true;
        }
        return false;
    }
    let words: Vec<&str> = lower.split_whitespace().collect();
    let number = |s: &str| s.parse::<f64>().ok().filter(|n| n.is_finite());
    match words.as_slice() {
        ["between", a, "and", b] => match (number(a), number(b)) {
            (Some(a), Some(b)) if a <= b => {
                hint.min = Some(a);
                hint.max = Some(b);
                true
            }
            _ => false,
        },
        ["at", "least", a] => number(a).map(|a| hint.min = Some(a)).is_some(),
        ["at", "most", b] => number(b).map(|b| hint.max = Some(b)).is_some(),
        _ => false,
    }
}

// ---------------------------------------------------------------------------
// Navigation
// ---------------------------------------------------------------------------

fn item_at_mut<'a>(table: &'a mut dyn TableLike, path: &[Seg]) -> Option<&'a mut Item> {
    let (first, rest) = path.split_first()?;
    let Seg::Key(key) = first else { return None };
    let item = table.get_mut(key)?;
    if rest.is_empty() {
        return Some(item);
    }
    match &rest[0] {
        Seg::Index(i) => {
            let block = item.as_array_of_tables_mut()?.get_mut(*i)?;
            item_at_mut(block, &rest[1..])
        }
        Seg::Key(_) => item_at_mut(item.as_table_like_mut()?, rest),
    }
}

fn value_at_mut<'a>(doc: &'a mut DocumentMut, path: &[Seg]) -> Result<&'a mut Value, EditError> {
    let name = path_text(path);
    match item_at_mut(doc.as_table_mut(), path) {
        Some(Item::Value(v)) => Ok(v),
        _ => Err(EditError::NotFound(name)),
    }
}

// ---------------------------------------------------------------------------
// Edits
// ---------------------------------------------------------------------------

/// Checks that `new` may replace a stored value of kind `old` and returns the
/// value to store: an integer into a float key becomes a float.
fn coerce(key: &str, old: ScalarKind, new: &Scalar) -> Result<Scalar, EditError> {
    match (old, new) {
        (ScalarKind::Float, Scalar::Integer(i)) => Ok(Scalar::Float(*i as f64)),
        _ if old == new.kind() => Ok(new.clone()),
        _ => Err(EditError::TypeMismatch {
            key: key.to_string(),
            expected: old.name(),
            found: new.kind().name(),
        }),
    }
}

fn build_value(key: &str, scalar: &Scalar) -> Result<Value, EditError> {
    scalar
        .to_value()
        .ok_or_else(|| EditError::BadDate(key.to_string()))
}

fn replace_keeping_decor(slot: &mut Value, mut new: Value) {
    *new.decor_mut() = slot.decor().clone();
    *slot = new;
}

impl SettingsDoc {
    /// The entry at `path`, if any.
    pub fn entry(&self, path: &[Seg]) -> Option<&Entry> {
        self.sections
            .iter()
            .flat_map(|s| s.entries.iter())
            .find(|e| e.path == path)
    }

    /// Sets a scalar key. The type must match (an integer is accepted for a
    /// float key and written as a float). Returns the new full text.
    pub fn set_scalar(&self, path: &[Seg], new: &Scalar) -> Result<String, EditError> {
        let key = path_text(path);
        let mut doc = self.doc.clone();
        let slot = value_at_mut(&mut doc, path)?;
        let old = Scalar::from_value(slot).ok_or_else(|| EditError::NotFound(key.clone()))?;
        let scalar = coerce(&key, old.kind(), new)?;
        replace_keeping_decor(slot, build_value(&key, &scalar)?);
        Ok(render(&self.source, &doc))
    }

    /// Sets element `index` of an array key.
    pub fn set_element(
        &self,
        path: &[Seg],
        index: usize,
        new: &Scalar,
    ) -> Result<String, EditError> {
        let key = path_text(path);
        let mut doc = self.doc.clone();
        let array = array_at_mut(&mut doc, path)?;
        let ElementKind::Uniform(kind) = array_kind(array) else {
            return Err(EditError::NotAnArray(key));
        };
        let scalar = coerce(&key, kind, new)?;
        let value = build_value(&key, &scalar)?;
        let slot = array
            .get_mut(index)
            .ok_or_else(|| EditError::NoSuchElement(key.clone()))?;
        replace_keeping_decor(slot, value);
        Ok(render(&self.source, &doc))
    }

    /// Appends a copy of the last element (so the new one has the same type).
    /// An empty array is an error: there is no type to copy.
    pub fn append_element(&self, path: &[Seg]) -> Result<String, EditError> {
        let key = path_text(path);
        let mut doc = self.doc.clone();
        let array = array_at_mut(&mut doc, path)?;
        match array_kind(array) {
            ElementKind::Empty => return Err(EditError::EmptyArray(key)),
            ElementKind::Mixed => return Err(EditError::NotAnArray(key)),
            ElementKind::Uniform(_) => {}
        }
        let Some(last) = array.iter().last() else {
            return Err(EditError::EmptyArray(key));
        };
        let mut copy = last.clone();
        copy.decor_mut().clear();
        array.push(copy);
        Ok(render(&self.source, &doc))
    }

    /// Removes element `index` of an array key.
    pub fn remove_element(&self, path: &[Seg], index: usize) -> Result<String, EditError> {
        let key = path_text(path);
        let mut doc = self.doc.clone();
        let array = array_at_mut(&mut doc, path)?;
        if matches!(array_kind(array), ElementKind::Mixed) {
            return Err(EditError::NotAnArray(key));
        }
        if index >= array.len() {
            return Err(EditError::NoSuchElement(key));
        }
        let first_prefix = array
            .get(0)
            .and_then(|v| v.decor().prefix().cloned())
            .filter(|_| index == 0);
        array.remove(index);
        if let (Some(prefix), Some(next)) = (first_prefix, array.get_mut(0)) {
            next.decor_mut().set_prefix(prefix);
        }
        Ok(render(&self.source, &doc))
    }

    /// The document as a neutral ordered tree, for the Python bridge.
    pub fn to_values(&self) -> SettingsValue {
        table_values(self.doc.as_table())
    }
}

/// The document as a neutral ordered tree. See [`SettingsDoc::to_values`].
pub fn to_values(doc: &SettingsDoc) -> SettingsValue {
    doc.to_values()
}

/// The edited document as text. `toml_edit` ends every line with a newline;
/// a source that did not end with one keeps it that way, so the only bytes
/// that change are the edited ones. That holds for LF text; in a CRLF source
/// every line ending outside multi-line strings becomes LF, which is why the
/// commands store LF ([`normalize_newlines`]).
fn render(source: &str, doc: &DocumentMut) -> String {
    let mut out = doc.to_string();
    if !source.is_empty() && !source.ends_with('\n') && out.ends_with('\n') {
        out.pop();
    }
    out
}

fn array_at_mut<'a>(doc: &'a mut DocumentMut, path: &[Seg]) -> Result<&'a mut Array, EditError> {
    let key = path_text(path);
    match value_at_mut(doc, path)? {
        Value::Array(a) => Ok(a),
        _ => Err(EditError::NotAnArray(key)),
    }
}

fn array_kind(array: &Array) -> ElementKind {
    match read_array(array) {
        EntryValue::Array { kind, .. } => kind,
        _ => ElementKind::Mixed,
    }
}

// ---------------------------------------------------------------------------
// To values
// ---------------------------------------------------------------------------

fn table_values(table: &dyn TableLike) -> SettingsValue {
    SettingsValue::Map(
        table
            .iter()
            .filter_map(|(k, item)| Some((k.to_string(), item_values(item)?)))
            .collect(),
    )
}

fn item_values(item: &Item) -> Option<SettingsValue> {
    match item {
        Item::None => None,
        Item::Value(v) => Some(value_values(v)),
        Item::Table(t) => Some(table_values(t)),
        Item::ArrayOfTables(blocks) => Some(SettingsValue::List(
            blocks.iter().map(|t| table_values(t)).collect(),
        )),
    }
}

fn value_values(value: &Value) -> SettingsValue {
    match value {
        Value::Array(a) => SettingsValue::List(a.iter().map(value_values).collect()),
        Value::InlineTable(t) => table_values(t),
        other => match Scalar::from_value(other) {
            Some(s) => SettingsValue::Scalar(s),
            None => SettingsValue::List(Vec::new()),
        },
    }
}

// ---------------------------------------------------------------------------
// Apply values (the script write)
// ---------------------------------------------------------------------------

/// Writes `values` (a map) into `text` and returns the new full text:
/// existing keys take the new value (same type rule as the form, an integer
/// accepted for a float key), missing keys are created at the end of their
/// table (a new table when needed), keys absent from `values` are untouched,
/// comments are kept. Hints never refuse a write. On any error nothing is
/// returned, so nothing is written.
pub fn apply_values(text: &str, values: &SettingsValue) -> Result<String, EditError> {
    let parsed = parse_inner(text)?;
    let mut doc = parsed.doc;
    let SettingsValue::Map(map) = values else {
        return Err(EditError::Unsupported("(root)".into()));
    };
    apply_map(doc.as_table_mut(), map, "")?;
    Ok(render(&parsed.source, &doc))
}

fn join(prefix: &str, key: &str) -> String {
    if prefix.is_empty() {
        key.to_string()
    } else {
        format!("{prefix}.{key}")
    }
}

fn apply_map(
    table: &mut dyn TableLike,
    map: &[(String, SettingsValue)],
    prefix: &str,
) -> Result<(), EditError> {
    for (key, value) in map {
        let name = join(prefix, key);
        match table.get_mut(key) {
            Some(item) => apply_existing(item, value, &name)?,
            None => {
                table.insert(key, build_item(value, &name)?);
            }
        }
    }
    Ok(())
}

fn apply_existing(item: &mut Item, value: &SettingsValue, name: &str) -> Result<(), EditError> {
    match (item, value) {
        (Item::Value(slot), SettingsValue::Scalar(new)) => {
            let old =
                Scalar::from_value(slot).ok_or_else(|| mismatch(name, "table or array", new))?;
            let scalar = coerce(name, old.kind(), new)?;
            replace_keeping_decor(slot, build_value(name, &scalar)?);
            Ok(())
        }
        (Item::Value(Value::Array(array)), SettingsValue::List(list)) => {
            let old = array_kind(array);
            let new = scalar_list(list, name)?;
            let mut items = Vec::new();
            for scalar in &new {
                let scalar = match old {
                    ElementKind::Uniform(kind) => coerce(name, kind, scalar)?,
                    _ => scalar.clone(),
                };
                items.push(build_value(name, &scalar)?);
            }
            let mut fresh = Array::new();
            for v in items {
                fresh.push(v);
            }
            *fresh.decor_mut() = array.decor().clone();
            *array = fresh;
            Ok(())
        }
        (Item::ArrayOfTables(blocks), SettingsValue::List(list)) => {
            if blocks.len() != list.len() {
                return Err(EditError::Unsupported(format!(
                    "{name}: {} blocks stored, {} given (add or remove blocks in the text)",
                    blocks.len(),
                    list.len()
                )));
            }
            for (block, new) in blocks.iter_mut().zip(list) {
                let SettingsValue::Map(map) = new else {
                    return Err(EditError::Unsupported(name.to_string()));
                };
                apply_map(block, map, name)?;
            }
            Ok(())
        }
        (item, SettingsValue::Map(map)) => match item.as_table_like_mut() {
            Some(table) => apply_map(table, map, name),
            None => Err(EditError::Unsupported(format!("{name}: not a table"))),
        },
        (_, SettingsValue::Scalar(new)) => Err(mismatch(name, "table or array", new)),
        (_, SettingsValue::List(_)) => Err(EditError::Unsupported(format!("{name}: not an array"))),
    }
}

fn mismatch(name: &str, expected: &'static str, new: &Scalar) -> EditError {
    EditError::TypeMismatch {
        key: name.to_string(),
        expected,
        found: new.kind().name(),
    }
}

fn scalar_list(list: &[SettingsValue], name: &str) -> Result<Vec<Scalar>, EditError> {
    let mut out: Vec<Scalar> = Vec::new();
    for v in list {
        let SettingsValue::Scalar(s) = v else {
            return Err(EditError::Unsupported(name.to_string()));
        };
        if out.first().is_some_and(|f| f.kind() != s.kind()) {
            return Err(EditError::Unsupported(format!(
                "{name}: the list mixes value types"
            )));
        }
        out.push(s.clone());
    }
    Ok(out)
}

fn build_item(value: &SettingsValue, name: &str) -> Result<Item, EditError> {
    match value {
        SettingsValue::Scalar(s) => Ok(Item::Value(build_value(name, s)?)),
        SettingsValue::List(list) => {
            if !list.is_empty() && list.iter().all(|v| matches!(v, SettingsValue::Map(_))) {
                let mut blocks = ArrayOfTables::new();
                for v in list {
                    if let Item::Table(t) = build_item(v, name)? {
                        blocks.push(t);
                    }
                }
                return Ok(Item::ArrayOfTables(blocks));
            }
            let mut array = Array::new();
            for s in scalar_list(list, name)? {
                array.push(build_value(name, &s)?);
            }
            Ok(Item::Value(Value::Array(array)))
        }
        SettingsValue::Map(map) => {
            let mut table = Table::new();
            apply_map(&mut table, map, name)?;
            let only_tables = table
                .iter()
                .all(|(_, i)| i.is_table() || i.is_array_of_tables());
            table.set_implicit(only_tables);
            Ok(Item::Table(table))
        }
    }
}

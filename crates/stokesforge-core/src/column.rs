//! Column of the data model: a named vector of values (the name lives
//! in the tree node, not here) of free length, plus its display and plot
//! metadata.
//!
//! Three architecture decisions shape this module: missing
//! values are `f64::NAN` for numeric/date-time columns and
//! `None` for text columns (distinct from `Some(String::new())`); the
//! buffers are contiguous `Vec`s (never chunked) shared behind an
//! `Arc`, mutated through `Arc::make_mut` so that the Python bridge can hold
//! a numpy view without copying while it holds a clone of the `Arc`; and
//! each column carries a **modification counter** ([`Column::version`])
//! that tells whether it changed without rereading its values.
//!
//! # Mixed-type columns
//!
//! [`ColumnType::Mixed`] is the fourth type, and the only one whose type
//! **is not that of the column but that of the cell**: its buffer
//! is a `Vec<`[`MixedCell`]`>`, where each row is independently empty, a
//! number or a text. This is what lets a column of a real import
//! carry `1,2  1,3  n/a  1,5` without choosing between "all numbers" (the
//! `n/a` becomes a missing value and the information is lost) and "all
//! text" (nothing can be plotted any more).
//!
//! Three decisions to know before touching it:
//!
//! - **The three homogeneous types stay fast paths, not special
//!   cases of a general case.** Making *every* column a
//!   `Vec<MixedCell>` would cost 32 bytes per row instead of 8 (the size
//!   of a `MixedCell` is that of a `String` plus its discriminant) and,
//!   above all, would remove the contiguous `Arc<Vec<f64>>` buffer on which
//!   rest the Python bridge's zero-copy numpy view and the zero-copy
//!   resolution of a series (`crate::resolve`). `Mixed` is therefore one more
//!   type, paid for when it is used, never a rework of the other
//!   three.
//! - **The missing value of a mixed column is [`MixedCell::Empty`]**,
//!   distinct from `MixedCell::Number(f64::NAN)` (a deliberately written `NaN`)
//!   as from `MixedCell::Text(String::new())` (a deliberate empty string) —
//!   the same distinction as `None` versus `Some("")` for a text column.
//!   The three count as "missing" on display ([`Cell::
//!   is_missing`]), not in storage.
//! - **A mixed column is not a text column.** It can be plotted:
//!   [`Column::numeric_view`] draws from it an `f64` view where text and
//!   empty cells are `NaN`, so that only numeric cells are plotted
//!   (the renderer already ignores non-finite points). This view is the only one
//!   in the whole module that **copies** — see its documentation.

use std::sync::Arc;

use crate::error::{CoreError, CoreResult};
use crate::formula::ColumnFormula;

/// Stored data type. `Numeric` and `DateTime` share the `f64`
/// representation (for `DateTime`, a POSIX timestamp in seconds); `Mixed` carries the
/// type **per cell** (see the module's top-level doc).
///
/// There is no longer a homogeneous `Text` type: a text column is a
/// `Mixed` column whose every cell is [`MixedCell::Text`] — functionally
/// equivalent (same memory footprint per cell as the former
/// `Option<String>`), and which avoids a fourth homogeneous type to maintain
/// separately. A `.stf` file written before this change still carries
/// the old `Text` tag: `stokesforge-persistence` rereads it and migrates the
/// column to `Mixed` (see `crates/stokesforge-persistence/src/convert.rs`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ColumnType {
    /// Numeric values (`f64`), missing = `NaN`.
    Numeric,
    /// POSIX timestamps in seconds (`f64`), missing = `NaN`.
    DateTime,
    /// Mixed types: each row is independently empty, a number or a
    /// text ([`MixedCell`]). Missing = [`MixedCell::Empty`].
    Mixed,
}

impl ColumnType {
    /// True if the underlying buffer is a contiguous `Vec<f64>` — hence if the
    /// column has a [`Column::float_values`] and can feed a numpy view
    /// without copying. False for `Mixed`.
    pub fn is_float_backed(self) -> bool {
        matches!(self, ColumnType::Numeric | ColumnType::DateTime)
    }

    /// Name of the type, for messages meant for a human —
    /// counterpart of [`crate::NodeKind::type_name`]. One place only, so that
    /// two messages do not name the same type differently.
    pub fn type_name(self) -> &'static str {
        match self {
            ColumnType::Numeric => "numeric",
            ColumnType::DateTime => "date/time",
            ColumnType::Mixed => "mixed",
        }
    }
}

/// A cell of a mixed-type column ([`ColumnType::Mixed`]): the type
/// is carried by the row, not by the column.
///
/// `Empty` is the missing value of this column type, **distinct** from
/// `Number(f64::NAN)` and `Text(String::new())`: all three display
/// as an empty cell ([`Cell::is_missing`]), but only `Number` and
/// `Text` keep in memory the trace of a deliberate input — it is the same
/// distinction as `None` versus `Some("")` for a text column.
#[derive(Debug, Clone, Default)]
pub enum MixedCell {
    /// Empty cell: the missing value of a mixed column.
    #[default]
    Empty,
    /// A number (or a POSIX timestamp, as for `DateTime`).
    Number(f64),
    /// A text. `Text(String::new())` is a deliberate empty string,
    /// distinct from [`MixedCell::Empty`].
    Text(String),
}

impl PartialEq for MixedCell {
    /// Stored-content equality, not IEEE equality: `f64`s are compared
    /// by bit pattern (`to_bits`), so `NaN == NaN` and `0.0 != -0.0` —
    /// same rule as [`Cell`], [`ValueBlock`] and [`ColumnValues`].
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (MixedCell::Empty, MixedCell::Empty) => true,
            (MixedCell::Number(a), MixedCell::Number(b)) => a.to_bits() == b.to_bits(),
            (MixedCell::Text(a), MixedCell::Text(b)) => a == b,
            _ => false,
        }
    }
}

impl Eq for MixedCell {}

impl MixedCell {
    /// The numeric value of the cell, or `NaN` if it carries none.
    ///
    /// A text is **not reparsed** here: this reads an already
    /// stored value, it is not a conversion. Promoting a numeric text to a
    /// number happens once and for all at write time
    /// ([`MixedCell::from_text`]) or at type change
    /// ([`Column::set_column_type`]).
    pub fn as_number(&self) -> f64 {
        match self {
            MixedCell::Number(value) => *value,
            MixedCell::Empty | MixedCell::Text(_) => f64::NAN,
        }
    }

    /// True if the cell counts as missing on display: empty, `NaN`,
    /// or empty string — same rule as [`Cell::is_missing`].
    pub fn is_missing(&self) -> bool {
        match self {
            MixedCell::Empty => true,
            MixedCell::Number(value) => value.is_nan(),
            MixedCell::Text(text) => text.is_empty(),
        }
    }

    /// The cell of a typed-in text: `None` (cleared cell) gives `Empty`,
    /// a text that parses as a finite number gives `Number`, everything else
    /// gives `Text`.
    ///
    /// The parsing is that of [`parse_f64_text`], shared with
    /// `Text → Numeric` (type conversion through the header): `inf` and `nan`
    /// are read as written there (rule C2). It is **not** the rule of
    /// typing or pasting, which go through [`ValueBlock::from_texts`] (#258)
    /// and keep `inf` / `nan` as text.
    pub fn from_text(text: Option<String>) -> Self {
        match text {
            None => MixedCell::Empty,
            Some(text) => match parse_f64_text(&text) {
                Some(value) => MixedCell::Number(value),
                None => MixedCell::Text(text),
            },
        }
    }

    /// The cell of a numeric value: `NaN` — the missing value of
    /// `f64` columns — becomes `Empty`, everything else becomes `Number`.
    pub fn from_f64(value: f64) -> Self {
        if value.is_nan() {
            MixedCell::Empty
        } else {
            MixedCell::Number(value)
        }
    }

    /// The written form of the cell, for `Mixed → Text`: `None` for an
    /// empty cell, the text as is for a text, and the form chosen by
    /// [`format_f64_for_display`] for a number.
    pub fn into_text(self) -> Option<String> {
        match self {
            MixedCell::Empty => None,
            MixedCell::Number(value) if value.is_nan() => None,
            MixedCell::Number(value) => Some(format_f64_for_display(value)),
            MixedCell::Text(text) => Some(text),
        }
    }
}

/// Role of the column in a plot, Origin-style.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum ColumnRole {
    /// Abscissa.
    X,
    /// Ordinate (default role of a newly created column).
    #[default]
    Y,
    /// Abscissa of the second layer (#167): X axis on top of the graph.
    X2,
    /// Ordinate of the second layer (#167), paired with the nearest X2 column
    /// to its left: Y axis on the right of the graph.
    Y2,
    /// Abscissa of the third layer (#168), above that of X2.
    X3,
    /// Ordinate of the third layer (#168), paired with the nearest X3 column
    /// to its left.
    Y3,
    /// Abscissa of the fourth layer (#168).
    X4,
    /// Ordinate of the fourth layer (#168).
    Y4,
    /// Third axis (surfaces, matrices shown as columns).
    Z,
    /// Error bar in X.
    XError,
    /// Error bar in Y.
    YError,
    /// Text label attached to the points.
    Label,
    /// No plot role (auxiliary column).
    None,
}

impl ColumnRole {
    /// All roles, in the order the "Set as" menu offers them.
    pub const ALL: [ColumnRole; 13] = [
        ColumnRole::X,
        ColumnRole::Y,
        ColumnRole::X2,
        ColumnRole::Y2,
        ColumnRole::X3,
        ColumnRole::Y3,
        ColumnRole::X4,
        ColumnRole::Y4,
        ColumnRole::Z,
        ColumnRole::XError,
        ColumnRole::YError,
        ColumnRole::Label,
        ColumnRole::None,
    ];

    /// The number of the layer this role feeds (#168): 1 for X/Y, 2 to 4
    /// for Xn/Yn, `None` for the other roles.
    pub fn layer(self) -> Option<u8> {
        match self {
            ColumnRole::X | ColumnRole::Y => Some(1),
            ColumnRole::X2 | ColumnRole::Y2 => Some(2),
            ColumnRole::X3 | ColumnRole::Y3 => Some(3),
            ColumnRole::X4 | ColumnRole::Y4 => Some(4),
            _ => None,
        }
    }

    /// The abscissa role that goes with an ordinate role (Y → X, Y3 → X3…),
    /// `None` for any other role.
    pub fn paired_x(self) -> Option<ColumnRole> {
        match self {
            ColumnRole::Y => Some(ColumnRole::X),
            ColumnRole::Y2 => Some(ColumnRole::X2),
            ColumnRole::Y3 => Some(ColumnRole::X3),
            ColumnRole::Y4 => Some(ColumnRole::X4),
            _ => None,
        }
    }
}

/// Display and plot metadata of a column (excluding name and type).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ColumnMeta {
    /// Plot role (X, Y, Z, error bar, label, ...).
    pub role: ColumnRole,
    /// Displayed unit (`"ms"`, `"°C"`, ...), free.
    pub unit: String,
    /// Long name / description, distinct from the short name carried by the node.
    pub long_name: String,
    /// Free comment.
    pub comment: String,
    /// Value of the special "x'" row (#164): an abscissa of the
    /// column's own — typically the instant of a spectrum in a kinetic —,
    /// which plays the role of a second X when plotting a **row** of
    /// data. `None`: x' row empty for this column. Always finite
    /// (the interface layer refuses NaN and infinity).
    pub x_prime: Option<f64>,
    /// Display format (free string interpreted by the UI layer).
    pub display_format: String,
    /// Calculation formula (`A+B`, `(A-B)*2`, ... on screen), `None` for an
    /// ordinary column. A column whose formula is `Some(_)` is
    /// recalculated automatically — see `stokesforge_core::formula` for
    /// the evaluator and `stokesforge-session::actor` for the orchestration of
    /// the recalculation — and refuses a direct write (`Command::SetCells`, see
    /// `stokesforge-commands`): its values are never the source of
    /// truth, only the formula is. It holds the columns it reads **by
    /// identifier** (#321): the displayed letters are obtained through
    /// [`crate::Tree::formula_letters`], and typed in through
    /// [`crate::Tree::formula_from_letters`]. `#[serde(default)]` on the
    /// persistence side (`stokesforge-persistence`): a file written before this
    /// field has only ordinary columns.
    pub formula: Option<ColumnFormula>,
    /// Metadata of the original file (header, and footer if the
    /// recipe asks for it), kept at import — see [`MetaLine`]. Empty
    /// for a hand-typed column. Read-only on the interface side: only the
    /// initial write (the "Import" macro) sets them, there is no edit
    /// command.
    pub file_metadata: Vec<MetaLine>,
    /// Where the values come from when the column was imported from a file:
    /// what is needed to **refresh** them from that file (equivalent of Origin's
    /// *Data Connector*). `None` for a hand-typed column.
    pub import_source: Option<ImportSource>,
}

/// Source of an imported column: the file, the recipe that read it
/// (serialized as JSON, the model does not know `stokesforge-import`) and the
/// position of the column in the file.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ImportSource {
    /// Path of the source file.
    pub path: String,
    /// The import recipe, as JSON.
    pub recipe_json: String,
    /// Index of the column in the file (0 = the first).
    pub column: usize,
    /// Rank of the data block in the file (0 = the first), for a
    /// multi-block file.
    pub block: usize,
}

/// A file metadata line, split into key / value
/// (`XUNITS;NANOMETERS` → `XUNITS` | `NANOMETERS`). A `[Section]` line is
/// a key in brackets with no value; a line that does not have the
/// key/value form is kept as is, in `value`, with an empty key.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MetaLine {
    /// Key (empty for a line with no key/value form).
    pub key: String,
    /// Value, or the whole line if the key is empty.
    pub value: String,
}

impl MetaLine {
    /// True for a section title (`[Measurement Information]`).
    pub fn is_section(&self) -> bool {
        self.value.is_empty() && self.key.starts_with('[') && self.key.ends_with(']')
    }
}

/// Typed block of values, as written into a column or as
/// recovered in an undo delta.
#[derive(Debug, Clone)]
pub enum ValueBlock {
    /// Numeric or date-time values.
    Float(Vec<f64>),
    /// Mixed-type values, one [`MixedCell`] per row.
    Mixed(Vec<MixedCell>),
}

impl PartialEq for ValueBlock {
    /// Stored-content equality, not IEEE equality: `f64`s are compared by
    /// bit pattern (`to_bits`), so `NaN == NaN` and `0.0 != -0.0`.
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (ValueBlock::Float(a), ValueBlock::Float(b)) => {
                a.len() == b.len() && a.iter().zip(b).all(|(x, y)| x.to_bits() == y.to_bits())
            }
            (ValueBlock::Mixed(a), ValueBlock::Mixed(b)) => a == b,
            _ => false,
        }
    }
}

impl Eq for ValueBlock {}

impl ValueBlock {
    /// Number of values in the block.
    pub fn len(&self) -> usize {
        match self {
            ValueBlock::Float(values) => values.len(),
            ValueBlock::Mixed(values) => values.len(),
        }
    }

    /// True if the block holds no value.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Empty block of the given column type.
    pub fn empty_for(column_type: ColumnType) -> Self {
        match column_type {
            ColumnType::Numeric | ColumnType::DateTime => ValueBlock::Float(Vec::new()),
            ColumnType::Mixed => ValueBlock::Mixed(Vec::new()),
        }
    }

    /// `count` missing values of the given type.
    pub fn missing_for(column_type: ColumnType, count: usize) -> Self {
        match column_type {
            ColumnType::Numeric | ColumnType::DateTime => ValueBlock::Float(vec![f64::NAN; count]),
            ColumnType::Mixed => ValueBlock::Mixed(vec![MixedCell::Empty; count]),
        }
    }

    /// The column type this block can feed directly, or `None`
    /// for a `Float` block — which feeds two (`Numeric` and `DateTime`)
    /// and so does not designate a type by itself. Used by
    /// [`ValueBlock::fits`].
    fn sole_column_type(&self) -> Option<ColumnType> {
        match self {
            ValueBlock::Float(_) => None,
            ValueBlock::Mixed(_) => Some(ColumnType::Mixed),
        }
    }

    /// True if this block can be written as is into a column of type
    /// `column_type` — the exact condition that [`Column::set_values`] checks
    /// before refusing with `CoreError::TypeMismatch`. `pub` because a
    /// caller that knows in advance that a block does not fit can choose to
    /// convert rather than take a refusal (this is what
    /// `Command::SetCellsPromoting` does, in `stokesforge-commands`).
    pub fn fits(&self, column_type: ColumnType) -> bool {
        match self.sole_column_type() {
            Some(sole) => sole == column_type,
            None => column_type.is_float_backed(),
        }
    }

    /// **The** rule "typed or pasted text → cell value" (#258), for
    /// a column of type `kind`: typing in one cell and pasting a
    /// whole column both go through here, so the same sequence of
    /// texts gives the same cells, cell by cell or in a single block.
    ///
    /// Each text is trimmed. An empty cell is a **missing value**
    /// (`NaN` in a `Float` block, [`MixedCell::Empty`] in a `Mixed`
    /// block). A cell is read as a **finite** number by
    /// [`crate::number_format::parse_input`] (decimal comma allowed); for
    /// `DateTime` only, a date or a time
    /// ([`crate::datetime::parse_datetime`]) is read first, as a POSIX
    /// timestamp. What cannot be read stays trimmed `Text` — `inf`, `nan` and
    /// `1e400` included: a typed or pasted infinity would silently skew
    /// automatic axes, integrals and fits (type conversion through
    /// the header, [`Column::set_column_type`], keeps its rule C2: a written
    /// `inf` is an intended infinity there).
    ///
    /// - `Numeric` / `DateTime`: `Float` block if all cells can be read
    ///   ; otherwise a `Mixed` block where each cell read becomes a `Number` (a
    ///   date stays a value) and the boolean is `true`: the caller must
    ///   write through `Command::SetCellsPromoting`, which widens the column to
    ///   `Mixed`.
    /// - `Mixed`: always a `Mixed` block, the same cell-by-cell reading without
    ///   dates, boolean `false` (nothing to widen).
    pub fn from_texts(kind: ColumnType, texts: &[&str]) -> (ValueBlock, bool) {
        let cells: Vec<Option<TextCell>> = texts
            .iter()
            .map(|text| {
                let trimmed = text.trim();
                (!trimmed.is_empty()).then(|| match parse_cell_number(kind, trimmed) {
                    Some(value) => TextCell::Number(value),
                    None => TextCell::Text(trimmed),
                })
            })
            .collect();
        let promoting = kind != ColumnType::Mixed
            && cells
                .iter()
                .any(|cell| matches!(cell, Some(TextCell::Text(_))));
        if kind != ColumnType::Mixed && !promoting {
            let values = cells
                .iter()
                .map(|cell| match cell {
                    Some(TextCell::Number(value)) => *value,
                    _ => f64::NAN,
                })
                .collect();
            return (ValueBlock::Float(values), false);
        }
        let cells = cells
            .iter()
            .map(|cell| match cell {
                None => MixedCell::Empty,
                Some(TextCell::Number(value)) => MixedCell::Number(*value),
                Some(TextCell::Text(text)) => MixedCell::Text((*text).to_string()),
            })
            .collect();
        (ValueBlock::Mixed(cells), promoting)
    }

    /// The same block, expressed as mixed cells — lossless for `Mixed`
    /// (no conversion), and by the same rules as
    /// [`Column::set_column_type`] for the other two: `NaN` and `None`
    /// become [`MixedCell::Empty`], and a text that parses as a number
    /// **is promoted to a number** ([`MixedCell::from_text`]).
    pub fn into_mixed(self) -> Vec<MixedCell> {
        match self {
            ValueBlock::Float(values) => values.into_iter().map(MixedCell::from_f64).collect(),
            ValueBlock::Mixed(values) => values,
        }
    }
}

/// A text cell already read by [`ValueBlock::from_texts`].
enum TextCell<'a> {
    Number(f64),
    Text(&'a str),
}

/// The number carried by a **trimmed, non-empty** text typed or pasted into a
/// column of type `kind` — see [`ValueBlock::from_texts`]: a date
/// first for `DateTime` only, then a finite number.
fn parse_cell_number(kind: ColumnType, trimmed: &str) -> Option<f64> {
    if kind == ColumnType::DateTime {
        if let Some(value) = crate::datetime::parse_datetime(trimmed) {
            return Some(value);
        }
    }
    crate::number_format::parse_input(trimmed)
}

/// A cell read.
#[derive(Debug, Clone)]
pub enum Cell {
    /// Numeric or date-time value.
    Float(f64),
    /// Text value; `None` = missing.
    Text(Option<String>),
}

impl PartialEq for Cell {
    /// Stored-content equality, not IEEE equality: `f64`s are compared by
    /// bit pattern (`to_bits`), so `NaN == NaN` and `0.0 != -0.0`.
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Cell::Float(a), Cell::Float(b)) => a.to_bits() == b.to_bits(),
            (Cell::Text(a), Cell::Text(b)) => a == b,
            _ => false,
        }
    }
}

impl Eq for Cell {}

impl Cell {
    /// True if the value is missing: `NaN` for `Float`, `None` **or `Some("")`** for
    /// `Text` (an empty string is stored distinctly but counts as missing on
    /// display — this is the behavior of the Python version).
    pub fn is_missing(&self) -> bool {
        match self {
            Cell::Float(value) => value.is_nan(),
            Cell::Text(None) => true,
            Cell::Text(Some(text)) => text.is_empty(),
        }
    }
}

impl From<MixedCell> for Cell {
    /// The reading of a mixed cell in the vocabulary common to all
    /// column types: a number is a `Float`, a text a
    /// `Text(Some(_))`, and an empty cell a `Text(None)` — the form that
    /// all readers of [`Cell`] already treat as missing.
    ///
    /// The conversion **loses** the distinction between `Empty` and a text
    /// column with no value, because no reader of `Cell` needs it:
    /// both read as "cell without value". A caller that wants the
    /// cell as it is stored reads [`Column::cell`], not
    /// [`Column::get`].
    fn from(cell: MixedCell) -> Self {
        match cell {
            MixedCell::Empty => Cell::Text(None),
            MixedCell::Number(value) => Cell::Float(value),
            MixedCell::Text(text) => Cell::Text(Some(text)),
        }
    }
}

/// Underlying buffer, shared through `Arc`. This `Arc` is what the Python bridge
/// will clone to hold a live numpy view.
#[derive(Debug, Clone)]
pub enum ColumnValues {
    /// Shared numeric or date-time buffer.
    Float(Arc<Vec<f64>>),
    /// Shared mixed-type buffer, one [`MixedCell`] per row.
    Mixed(Arc<Vec<MixedCell>>),
}

impl PartialEq for ColumnValues {
    /// Stored-content equality, not IEEE equality: `f64`s are compared by
    /// bit pattern (`to_bits`), so `NaN == NaN` and `0.0 != -0.0`.
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (ColumnValues::Float(a), ColumnValues::Float(b)) => {
                a.len() == b.len()
                    && a.iter()
                        .zip(b.iter())
                        .all(|(x, y)| x.to_bits() == y.to_bits())
            }
            (ColumnValues::Mixed(a), ColumnValues::Mixed(b)) => a == b,
            _ => false,
        }
    }
}

impl Eq for ColumnValues {}

impl ColumnValues {
    /// Length of the buffer.
    pub fn len(&self) -> usize {
        match self {
            ColumnValues::Float(values) => values.len(),
            ColumnValues::Mixed(values) => values.len(),
        }
    }

    /// True if the buffer is empty.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Cell at row `row`; missing value beyond the length (never an error).
    pub fn get(&self, row: usize) -> Cell {
        match self {
            ColumnValues::Float(values) => {
                Cell::Float(values.get(row).copied().unwrap_or(f64::NAN))
            }
            ColumnValues::Mixed(values) => Cell::from(values.get(row).cloned().unwrap_or_default()),
        }
    }

    /// Cell at row `row`, **as it is stored** — the reading
    /// that does not lose the distinction between an empty cell and a text
    /// cell with no value (see `impl From<MixedCell> for Cell`).
    /// [`MixedCell::Empty`] beyond the length, never an error.
    pub fn cell(&self, row: usize) -> MixedCell {
        match self {
            ColumnValues::Float(values) => values
                .get(row)
                .copied()
                .map_or(MixedCell::Empty, MixedCell::from_f64),
            ColumnValues::Mixed(values) => values.get(row).cloned().unwrap_or_default(),
        }
    }

    /// The numeric value at row `row`, or `NaN` — with no allocation,
    /// whatever the column type.
    ///
    /// This is the reading of **calculations** (`crate::formula`), where [`Cell`]
    /// would clone a `String` per row only to throw it away. A text
    /// is never reparsed: see [`MixedCell::as_number`].
    pub fn number_at(&self, row: usize) -> f64 {
        match self {
            ColumnValues::Float(values) => values.get(row).copied().unwrap_or(f64::NAN),
            ColumnValues::Mixed(values) => values.get(row).map_or(f64::NAN, MixedCell::as_number),
        }
    }

    /// Numeric status of the cell at row `row` — counterpart of
    /// [`ColumnValues::number_at`], but which distinguishes **why** a
    /// cell carries no number: [`NumericStatus::Missing`] (nothing to
    /// compute: empty cell, or out of the buffer's bounds)
    /// versus [`NumericStatus::Failed`] (a non-empty, non-numeric text
    /// is present — a real calculation failure, not an absence). Used by
    /// `crate::formula` to tell, row by row, a formula that has
    /// simply nothing to do from a formula that really fails (shown
    /// `---` in the UI, against a cell left empty).
    ///
    pub fn status_at(&self, row: usize) -> NumericStatus {
        match self {
            ColumnValues::Float(values) => match values.get(row) {
                Some(value) if value.is_nan() => NumericStatus::Missing,
                Some(_) => NumericStatus::Ok,
                None => NumericStatus::Missing,
            },
            ColumnValues::Mixed(values) => match values.get(row) {
                None | Some(MixedCell::Empty) => NumericStatus::Missing,
                Some(MixedCell::Number(value)) if value.is_nan() => NumericStatus::Missing,
                Some(MixedCell::Number(_)) => NumericStatus::Ok,
                Some(MixedCell::Text(text)) if text.is_empty() => NumericStatus::Missing,
                Some(MixedCell::Text(_)) => NumericStatus::Failed,
            },
        }
    }
}

/// Status of a cell with respect to a numeric calculation — see
/// [`ColumnValues::status_at`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NumericStatus {
    /// A finite numeric value is present.
    Ok,
    /// Nothing at this row: empty cell, or row beyond the length of
    /// the column.
    Missing,
    /// A non-empty, non-numeric text occupies the cell: the calculation
    /// cannot be done, it is not a simple absence.
    Failed,
}

impl NumericStatus {
    /// Combines the statuses of two operands of the same binary operator:
    /// `Failed` always wins (a real failure must never hide
    /// behind an empty cell), otherwise `Missing` wins over `Ok`.
    pub fn combine(self, other: NumericStatus) -> NumericStatus {
        match (self, other) {
            (NumericStatus::Failed, _) | (_, NumericStatus::Failed) => NumericStatus::Failed,
            (NumericStatus::Missing, _) | (_, NumericStatus::Missing) => NumericStatus::Missing,
            (NumericStatus::Ok, NumericStatus::Ok) => NumericStatus::Ok,
        }
    }
}

/// What is needed to undo a `set_values` without copying the column: the
/// overwritten values and the previous length. Holds no `f64` of its
/// own (they are in `previous: ValueBlock`): deriving `PartialEq`/`Eq`
/// stays correct since it delegates to the bit-for-bit equality of `ValueBlock`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValuesDelta {
    /// Start row of the write that produced this delta.
    pub start: usize,
    /// Values overwritten by the write (the part covering rows already present).
    pub previous: ValueBlock,
    /// Length of the column before the write.
    pub previous_length: usize,
}

/// Data vector of free length plus its metadata. The name lives
/// in the tree node.
#[derive(Debug, Clone)]
pub struct Column {
    column_type: ColumnType,
    values: ColumnValues,
    meta: ColumnMeta,
    version: u64,
}

/// **The version counter is excluded from equality, deliberately.**
///
/// The invariant that `stokesforge-commands` tests is "after undo, the tree is
/// bit for bit the one from before"; yet undoing a write is *one more
/// write*, so it increments the counter. Including it would make this
/// property fail although the data are identical.
///
/// Consequence to know: a disk round trip is **not** covered
/// by this equality for the counter — hence a dedicated test in
/// `stokesforge-persistence`.
impl PartialEq for Column {
    fn eq(&self, other: &Self) -> bool {
        self.column_type == other.column_type
            && self.values == other.values
            && self.meta == other.meta
    }
}

/// B3 (`guide-de-correction-robustesse.md` §F4) — cap shared by
/// `write_block`, `insert_rows` and `set_length`: beyond it, sizing
/// the requested allocation causes `abort()` (allocation failure, not
/// recoverable — see `stokesforge-sonde-alloc`) before an error
/// has the slightest chance of being built. One hundred million rows,
/// arbitrary but documented: far beyond any realistic project, far
/// below what threatens the process (at 8 bytes/value, the allocation stays
/// around a gigabyte).
/// Cap common to model writes and to the copy of script inputs.
pub const MAX_COLUMN_ROWS: usize = 100_000_000;

/// Writes `new_values` into `vec` starting at `start`, extending `vec` with
/// missing values (`missing`) if needed. Returns the overwritten values
/// (only the portion of `new_values` that covers rows already
/// present) as well as the previous length of `vec`.
///
/// B3 — `Err(CoreError::IndexOutOfBounds)` if the resulting length
/// would exceed [`MAX_COLUMN_ROWS`], **before** any resizing: an
/// uncontrolled `start` or `new_values.len()` (`SetCells { start: 1<<45 }`)
/// must never reach `resize_with`/`extend`, the only way to avoid
/// the `abort()` of an allocation failure, which no `Result` catches
/// afterwards.
fn write_block<T: Clone>(
    vec: &mut Vec<T>,
    start: usize,
    new_values: Vec<T>,
    missing: impl Fn() -> T,
) -> CoreResult<(Vec<T>, usize)> {
    // C3 (`guide-de-correction-robustesse.md` §F5, trap in §6) — an empty
    // block has nothing to write: early exit rather than letting
    // `resize_with(start, missing)` below lengthen the column up to
    // `start` before discovering there is no value to place there. Returning
    // `Ok` with a neutral delta (`previous` empty, `previous_length`
    // unchanged) is necessary, not only sufficient:
    // `History::revert_delta` calls `set_values(start, before)` with an
    // empty `before` whenever a write made beyond
    // the end of the column is undone — an empty block that failed would break
    // the undo.
    if new_values.is_empty() {
        return Ok((Vec::new(), vec.len()));
    }
    let resulting_len = start.saturating_add(new_values.len()).max(vec.len());
    if resulting_len > MAX_COLUMN_ROWS {
        return Err(CoreError::IndexOutOfBounds(format!(
            "the write would grow the column to {resulting_len} rows, beyond the limit of {MAX_COLUMN_ROWS}"
        )));
    }
    let previous_length = vec.len();
    let overlap_start = start.min(previous_length);
    let overlap_end = previous_length.min(start.saturating_add(new_values.len()));
    let previous = vec
        .get(overlap_start..overlap_end)
        .map(|slice| slice.to_vec())
        .unwrap_or_default();

    if vec.len() < start {
        vec.resize_with(start, missing);
    }

    let mut incoming = new_values.into_iter();
    if let Some(tail) = vec.get_mut(start..) {
        for slot in tail.iter_mut() {
            match incoming.next() {
                Some(value) => *slot = value,
                None => break,
            }
        }
    }
    vec.extend(incoming);

    Ok((previous, previous_length))
}

/// Truncates or fills `vec` to `length`, filling any extension
/// with `missing`. Returns the removed tail (empty if extension or
/// unchanged length) and the previous length — the body common to the three
/// branches of [`Column::set_length`], one per buffer type.
///
/// Does not apply [`MAX_COLUMN_ROWS`]: the caller bounds `length`
/// before calling (see the doc of `set_length`, which must stay infallible).
fn resize_block<T: Clone>(vec: &mut Vec<T>, length: usize, missing: T) -> (Vec<T>, usize) {
    let previous_length = vec.len();
    if length < previous_length {
        return (vec.split_off(length), previous_length);
    }
    if length > previous_length {
        vec.resize(length, missing);
    }
    (Vec::new(), previous_length)
}

/// Parses a text into an `f64` value for `Column::set_column_type`
/// (`Text → Numeric`/`DateTime`). Returns `None` — same treatment as the model's
/// missing value — in two cases that this parser deliberately
/// tells apart:
///
/// - the text does not parse at all (`"not a number"`);
/// - the text parses as a **finite** number spelled out (digits,
///   point, exponent) but the value it names overflows `f64` — `"1e400"`
///   is syntactically a number, but no finite `f64` holds it.
///
/// A text that explicitly **asks** for a non-finite value — `"inf"`,
/// `"-Infinity"`, `"NaN"`, with or without sign, case-insensitive — is
/// not this overflow: the user wrote what they wanted, and it is handed back
/// as is (`"NaN"` incidentally becomes the model's missing value,
/// not an error: it is the same value, spelled out).
///
/// C2 (`guide-de-correction-robustesse.md` §F1, §F8): the repository's decision
/// for an unrepresentable input value is to refuse it rather than
/// convert it silently — it is already the rule for `int64` on the Python
/// bridge side. Here, "refusing" an overflow means: never produce an implicit
/// `+inf`/`-inf`, fall back to the model's missing value,
/// exactly as for a text that does not parse.
fn parse_f64_text(text: &str) -> Option<f64> {
    let trimmed = text.trim();
    let lower = trimmed.to_ascii_lowercase();
    match lower.as_str() {
        "inf" | "+inf" | "infinity" | "+infinity" => return Some(f64::INFINITY),
        "-inf" | "-infinity" => return Some(f64::NEG_INFINITY),
        "nan" | "+nan" | "-nan" => return Some(f64::NAN),
        _ => {}
    }
    // Real overflow (`"1e400"`) or parse failure: refused, never
    // silently `+inf`/`-inf`. The decimal comma is read (#223).
    crate::number_format::parse_input(trimmed)
}

/// Formats an `f64` for display or for `Column::set_column_type`
/// (`Numeric → Text`) — **the** written form of a model value (C4,
/// `guide-de-correction-robustesse.md` §F8): `Column::set_column_type` and
/// `AppState::cell_text` (`stokesforge-app`) must produce the same string for
/// the same value, otherwise display and conversion would
/// contradict each other.
///
/// Switches to scientific notation (`{v:e}`, never re-emitted by the
/// `Display` of `f64`) whenever it is shorter than the decimal
/// form: a measurement around 10⁻¹⁹ s is then written in about fifteen
/// characters rather than thirty, and an ordinary integer (`1`, `100`)
/// keeps its decimal form, shorter than its exponent (`1e0`, `1e2`).
///
/// Bit-exact on the round trip: `Display` and `LowerExp` of `f64`
/// each produce the shortest decimal representation that reparses
/// exactly to the original value (denormals and `-0.0` included); this
/// choice only selects which of the two forms to write.
pub fn format_f64_for_display(v: f64) -> String {
    if !v.is_finite() {
        // "NaN" never reaches this function in practice (it is the model's
        // missing value, translated to `None` before calling here);
        // ±inf, on the other hand, is a model value like any other.
        return v.to_string();
    }
    let decimal = v.to_string();
    let scientific = format!("{v:e}");
    if scientific.len() < decimal.len() {
        scientific
    } else {
        decimal
    }
}

impl Column {
    /// Empty column of the given type, default metadata.
    pub fn new(column_type: ColumnType) -> Self {
        let values = match column_type {
            ColumnType::Numeric | ColumnType::DateTime => ColumnValues::Float(Arc::new(Vec::new())),
            ColumnType::Mixed => ColumnValues::Mixed(Arc::new(Vec::new())),
        };
        Self {
            column_type,
            values,
            meta: ColumnMeta::default(),
            version: 0,
        }
    }

    /// Initialized numeric column (copies the slice).
    pub fn from_f64(values: &[f64]) -> Self {
        Self {
            column_type: ColumnType::Numeric,
            values: ColumnValues::Float(Arc::new(values.to_vec())),
            meta: ColumnMeta::default(),
            version: 0,
        }
    }

    /// Mixed column initialized from optional strings: `None`
    /// becomes [`MixedCell::Empty`], each text becomes [`MixedCell::Text`]
    /// **as is**, without being reparsed as a number (unlike
    /// [`ValueBlock::into_mixed`]) — it is the equivalent, for a mixed
    /// column, of the former homogeneous text column.
    pub fn from_text(values: &[Option<String>]) -> Self {
        let cells: Vec<MixedCell> = values
            .iter()
            .cloned()
            .map(|value| match value {
                None => MixedCell::Empty,
                Some(text) => MixedCell::Text(text),
            })
            .collect();
        Self::from_mixed(&cells)
    }

    /// Initialized mixed-type column (copies). Cells are taken
    /// **as is**: unlike [`ValueBlock::into_mixed`], nothing
    /// is promoted or parsed here, the caller has already decided each row.
    pub fn from_mixed(values: &[MixedCell]) -> Self {
        Self {
            column_type: ColumnType::Mixed,
            values: ColumnValues::Mixed(Arc::new(values.to_vec())),
            meta: ColumnMeta::default(),
            version: 0,
        }
    }

    /// Column from a typed block and metadata. `Err(TypeMismatch)` if the block does not
    /// match the type (`Float` for `Numeric`/`DateTime`, `Mixed` for `Mixed`).
    pub fn from_block(
        column_type: ColumnType,
        block: ValueBlock,
        meta: ColumnMeta,
    ) -> CoreResult<Self> {
        let values = match (column_type, block) {
            (ColumnType::Numeric | ColumnType::DateTime, ValueBlock::Float(values)) => {
                ColumnValues::Float(Arc::new(values))
            }
            (ColumnType::Mixed, ValueBlock::Mixed(values)) => ColumnValues::Mixed(Arc::new(values)),
            _ => {
                return Err(CoreError::TypeMismatch(
                    "the value block does not match the column type".to_string(),
                ))
            }
        };
        Ok(Self {
            column_type,
            values,
            meta,
            version: 0,
        })
    }

    /// Like [`Column::from_block`], but with an imposed version counter.
    /// Reserved for **reconstruction from a file**: the counter is
    /// persisted, and restarting it from zero at load would make the counter
    /// a liar (see [`Column::version`]).
    pub fn from_block_with_version(
        column_type: ColumnType,
        block: ValueBlock,
        meta: ColumnMeta,
        version: u64,
    ) -> CoreResult<Self> {
        let mut column = Self::from_block(column_type, block, meta)?;
        column.version = version;
        Ok(column)
    }

    /// Data type of the column.
    pub fn column_type(&self) -> ColumnType {
        self.column_type
    }

    /// Shared value buffer.
    pub fn values(&self) -> &ColumnValues {
        &self.values
    }

    /// Display and plot metadata.
    pub fn meta(&self) -> &ColumnMeta {
        &self.meta
    }

    /// Modification counter of the **data**: it answers "has this column
    /// changed since the last time I read it?" without rereading
    /// the values. A reader keeps the counter with what it produced and
    /// knows it is stale when it differs.
    ///
    /// # Exact rule
    ///
    /// Incremented by **every successful call** to a data mutator
    /// ([`Column::set_values`], [`Column::set_length`],
    /// [`Column::insert_rows`], [`Column::delete_rows`],
    /// [`Column::set_column_type`], [`Column::restore`]), **including when
    /// the call turns out to have no effect** (`insert_rows(_, 0)`, `set_length` to the
    /// same length). A refused call (`Err`) does not increment.
    ///
    /// [`Column::set_meta`] **does not increment**: a unit or a comment
    /// is not data.
    ///
    /// The direction of the approximation is deliberate: a false "has changed" costs
    /// a useless recalculation, a false "has not changed" would show a wrong
    /// result. Only the first error is tolerable.
    ///
    /// # Who relies on it
    ///
    /// The targeted recalculation of formula columns (`stokesforge-session`,
    /// `recalc.rs`, #193) recalculates a formula only if the version of one of
    /// its sources changed, and `Command::SetCells::expected_version` refuses
    /// a write based on a stale read. Both assume that a
    /// number never designates two contents of the same column: every
    /// write goes through `Project`, which replaces this counter with a
    /// fresh number from its single counter (`Project::next_column_version`), and a
    /// column inserted with its number moves that counter beyond it
    /// (`Project::insert_node`).
    ///
    /// # It is persisted
    ///
    /// Restarting from zero at load would make it a **liar**: after a
    /// reload, forty-seven writes would bring the counter back to a
    /// value that a reader from before the save would think it recognizes, on
    /// data that has since become entirely different. See
    /// [`Column::from_block_with_version`].
    ///
    /// # It is outside equality
    ///
    /// `PartialEq` ignores it — see the implementation and its justification.
    pub fn version(&self) -> u64 {
        self.version
    }

    /// Number of rows of the column.
    pub fn len(&self) -> usize {
        self.values.len()
    }

    /// True if the column holds no row.
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    /// Cell at `row`; missing out of bounds, never an error.
    pub fn get(&self, row: usize) -> Cell {
        self.values.get(row)
    }

    /// Cell at `row` **as it is stored** — see
    /// [`ColumnValues::cell`]. It is the reading to prefer whenever a mixed
    /// column is possible: [`Column::get`] conflates an empty cell and a
    /// text cell with no value.
    pub fn cell(&self, row: usize) -> MixedCell {
        self.values.cell(row)
    }

    /// True if the cell at `row` is missing (see `Cell::is_missing`).
    pub fn is_missing(&self, row: usize) -> bool {
        self.get(row).is_missing()
    }

    /// The shared `f64` buffer, or `None` for a mixed column.
    /// It is the `Arc` that the Python bridge clones to hold a view without copying —
    /// a mixed column has no contiguous `f64` buffer, hence no
    /// numpy view without copying (see [`Column::numeric_view`]).
    pub fn float_values(&self) -> Option<&Arc<Vec<f64>>> {
        match &self.values {
            ColumnValues::Float(values) => Some(values),
            ColumnValues::Mixed(_) => None,
        }
    }

    /// An `f64` view of the column for plotting.
    ///
    /// - `Numeric`/`DateTime`: the buffer's `Arc`, **without copying** — this is the
    ///   "zero copy" property of `crate::resolve`.
    /// - `Mixed`: a **new** buffer, where each numeric cell gives its
    ///   value and everything else (text, empty cell) gives `NaN`. The
    ///   renderer already discards non-finite points: the result is that a
    ///   mixed column plots exactly its numeric cells, without a
    ///   single backend having to know it (invariant 5).
    ///
    /// **It is the only read of the module that copies**, and it is the price of
    /// the per-cell representation: a `Vec<MixedCell>` is not a contiguous
    /// `Vec<f64>`, there is nothing to share. The cost is `O(n)` per
    /// resolution and per mixed column **referenced by a series** — not per
    /// column of the project — and it concerns no homogeneous column.
    pub fn numeric_view(&self) -> Option<Arc<Vec<f64>>> {
        match &self.values {
            ColumnValues::Float(values) => Some(Arc::clone(values)),
            ColumnValues::Mixed(values) => {
                Some(Arc::new(values.iter().map(MixedCell::as_number).collect()))
            }
        }
    }

    /// Writes `block` starting at row `start`, extending the column if needed;
    /// the extension fills any gaps with the missing value. Returns the overwritten
    /// values (only the part of `block` that covers existing rows) and the
    /// previous length. `Err(TypeMismatch)` if the block variant does not match the type.
    pub fn set_values(&mut self, start: usize, block: ValueBlock) -> CoreResult<ValuesDelta> {
        // Incremented only on the branches that write: the
        // `Err(TypeMismatch)` branch leaves the column — counter included — intact.
        match (&mut self.values, block) {
            (ColumnValues::Float(arc), ValueBlock::Float(new_values)) => {
                let vec = Arc::make_mut(arc);
                let (previous, previous_length) = write_block(vec, start, new_values, || f64::NAN)?;
                self.version = self.version.wrapping_add(1);
                Ok(ValuesDelta {
                    start,
                    previous: ValueBlock::Float(previous),
                    previous_length,
                })
            }
            (ColumnValues::Mixed(arc), ValueBlock::Mixed(new_values)) => {
                let vec = Arc::make_mut(arc);
                let (previous, previous_length) =
                    write_block(vec, start, new_values, || MixedCell::Empty)?;
                self.version = self.version.wrapping_add(1);
                Ok(ValuesDelta {
                    start,
                    previous: ValueBlock::Mixed(previous),
                    previous_length,
                })
            }
            _ => Err(CoreError::TypeMismatch(format!(
                "value block incompatible with the column type ({:?})",
                self.column_type
            ))),
        }
    }

    /// Truncates or fills to `length` rows. The delta carries the removed tail (empty if extension
    /// or unchanged), `start = length`, `previous` = removed values (empty if none),
    /// `previous_length` = old length.
    ///
    /// B3 (`guide-de-correction-robustesse.md` §F4): `length` is first bounded to
    /// [`MAX_COLUMN_ROWS`], so as never to attempt a `resize` that would size a
    /// catastrophic allocation (allocation failure → `abort()`, not recoverable — see
    /// `stokesforge-sonde-alloc`). Unlike [`Column::set_values`] and
    /// [`Column::insert_rows`], this method stays **infallible** by necessity: it is the
    /// primitive that `History::revert_delta` uses to restore a previous length
    /// during an undo, and undoing must never be able to fail. In practice a
    /// restored length has always already gone through `write_block`/`insert_rows`, so never
    /// exceeded the bound; the cap here protects a direct call of the core API with an
    /// arbitrary length (this is the case the probe covers).
    pub fn set_length(&mut self, length: usize) -> ValuesDelta {
        let length = length.min(MAX_COLUMN_ROWS);
        self.version = self.version.wrapping_add(1);
        match &mut self.values {
            ColumnValues::Float(arc) => {
                let (previous, previous_length) =
                    resize_block(Arc::make_mut(arc), length, f64::NAN);
                ValuesDelta {
                    start: length,
                    previous: ValueBlock::Float(previous),
                    previous_length,
                }
            }
            ColumnValues::Mixed(arc) => {
                let (previous, previous_length) =
                    resize_block(Arc::make_mut(arc), length, MixedCell::Empty);
                ValuesDelta {
                    start: length,
                    previous: ValueBlock::Mixed(previous),
                    previous_length,
                }
            }
        }
    }

    /// Inserts `count` missing rows starting at `at`. `at` must be in `0..=len`
    /// (equality inserts at the end) otherwise `Err(IndexOutOfBounds)`. B3 (`guide-de-correction-
    /// robustesse.md` §F4): also `Err(IndexOutOfBounds)` if the resulting length
    /// would exceed [`MAX_COLUMN_ROWS`] — an uncontrolled `count` must never reach
    /// `Vec::splice`, the only way to avoid the `abort()` of an allocation failure.
    pub fn insert_rows(&mut self, at: usize, count: usize) -> CoreResult<()> {
        let len = self.len();
        if at > len {
            return Err(CoreError::IndexOutOfBounds(format!(
                "row {at} out of bounds (length {len})"
            )));
        }
        let resulting_len = len.saturating_add(count);
        if resulting_len > MAX_COLUMN_ROWS {
            return Err(CoreError::IndexOutOfBounds(format!(
                "the insertion would grow the column to {resulting_len} rows, beyond the limit of {MAX_COLUMN_ROWS}"
            )));
        }
        self.version = self.version.wrapping_add(1);
        if count == 0 {
            return Ok(());
        }
        match &mut self.values {
            ColumnValues::Float(arc) => {
                let vec = Arc::make_mut(arc);
                vec.splice(at..at, std::iter::repeat_n(f64::NAN, count));
            }
            ColumnValues::Mixed(arc) => {
                let vec = Arc::make_mut(arc);
                vec.splice(at..at, std::iter::repeat_n(MixedCell::Empty, count));
            }
        }
        Ok(())
    }

    /// Deletes `count` rows starting at `at`, clamped to the end (`at + count > len` deletes
    /// up to the end, without error). `at` must be in `0..=len` otherwise `Err(IndexOutOfBounds)`.
    /// Returns the removed values.
    pub fn delete_rows(&mut self, at: usize, count: usize) -> CoreResult<ValueBlock> {
        let len = self.len();
        if at > len {
            return Err(CoreError::IndexOutOfBounds(format!(
                "row {at} out of bounds (length {len})"
            )));
        }
        self.version = self.version.wrapping_add(1);
        let stop = len.min(at.saturating_add(count));
        match &mut self.values {
            ColumnValues::Float(arc) => {
                let vec = Arc::make_mut(arc);
                Ok(ValueBlock::Float(vec.drain(at..stop).collect()))
            }
            ColumnValues::Mixed(arc) => {
                let vec = Arc::make_mut(arc);
                Ok(ValueBlock::Mixed(vec.drain(at..stop).collect()))
            }
        }
    }

    /// Replaces the metadata. Returns the previous ones.
    pub fn set_meta(&mut self, meta: ColumnMeta) -> ColumnMeta {
        std::mem::replace(&mut self.meta, meta)
    }

    /// Changes the type and converts the data. Returns `(old type, old values)` —
    /// a deliberate **snapshot**: the conversion is destructive in both directions.
    /// `Numeric ↔ DateTime`: no change of values (a number stays the
    /// POSIX timestamp it was, #218). Same type → no change, still returns
    /// `(type, clone of the values)`.
    ///
    /// # To and from `Mixed`
    ///
    /// - `Float → Mixed`: `NaN → Empty`, otherwise `Number(v)`. **Lossless** —
    ///   it is the conversion that a column promotion performs.
    /// - `Mixed → Float`: `Number(v) → v`, `Empty → NaN`, and a text is
    ///   parsed by [`parse_f64_text`] (`NaN` if parsing fails or overflows
    ///   — C2, `guide-de-correction-robustesse.md` §F1/§F8: never an implicit
    ///   `+inf`/`-inf`).
    pub fn set_column_type(&mut self, column_type: ColumnType) -> (ColumnType, ColumnValues) {
        let old_type = self.column_type;
        let old_values = self.values.clone();
        self.version = self.version.wrapping_add(1);

        if column_type == old_type {
            return (old_type, old_values);
        }

        let new_values = match (&self.values, column_type) {
            (ColumnValues::Float(values), ColumnType::Numeric | ColumnType::DateTime) => {
                ColumnValues::Float(Arc::clone(values))
            }
            (ColumnValues::Float(values), ColumnType::Mixed) => ColumnValues::Mixed(Arc::new(
                values.iter().copied().map(MixedCell::from_f64).collect(),
            )),
            (ColumnValues::Mixed(values), ColumnType::Numeric | ColumnType::DateTime) => {
                let converted = values
                    .iter()
                    .map(|cell| match cell {
                        MixedCell::Empty => f64::NAN,
                        MixedCell::Number(value) => *value,
                        // To Date/Time (#218), a text that is a date is read
                        // as a timestamp before being tried as a number.
                        MixedCell::Text(text) => (column_type == ColumnType::DateTime)
                            .then(|| crate::datetime::parse_datetime(text))
                            .flatten()
                            .or_else(|| parse_f64_text(text))
                            .unwrap_or(f64::NAN),
                    })
                    .collect();
                ColumnValues::Float(Arc::new(converted))
            }
            (ColumnValues::Mixed(values), ColumnType::Mixed) => {
                ColumnValues::Mixed(Arc::clone(values))
            }
        };

        self.column_type = column_type;
        self.values = new_values;

        (old_type, old_values)
    }

    /// Replaces the type and the values entirely (used to undo `set_column_type`).
    /// `Err(TypeMismatch)` if `values` does not match `column_type`.
    pub fn restore(&mut self, column_type: ColumnType, values: ColumnValues) -> CoreResult<()> {
        let matches_type = matches!(
            (column_type, &values),
            (
                ColumnType::Numeric | ColumnType::DateTime,
                ColumnValues::Float(_)
            ) | (ColumnType::Mixed, ColumnValues::Mixed(_))
        );
        if !matches_type {
            return Err(CoreError::TypeMismatch(format!(
                "the values do not match the type {column_type:?}"
            )));
        }
        self.column_type = column_type;
        self.values = values;
        self.version = self.version.wrapping_add(1);
        Ok(())
    }

    /// Imposes a version number, touching neither the values nor the type.
    ///
    /// Reserved for [`crate::Project::restamp_column_version`], itself
    /// reserved for `History::revert` in `stokesforge-commands` (A7,
    /// `guide-de-correction-robustesse.md` §F2): after a structure
    /// restoration (`replace_tree`) has brought an old counter
    /// back, assigning a **fresh** number (never seen) prevents a future
    /// write from landing, for this column, on a number already
    /// carried by different content — see the stamp doctrine of the
    /// guide (§F2: "a content version never designates two
    /// contents").
    ///
    /// Never clones the buffer (`self.values` is not touched): cost
    /// `O(1)`, the buffer stays shared with the tree that was just left.
    pub(crate) fn restamp_version(&mut self, version: u64) {
        self.version = version;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn values_of(column: &Column) -> Vec<f64> {
        column
            .float_values()
            .expect("numeric column")
            .as_slice()
            .to_vec()
    }

    #[test]
    fn new_numeric_column_is_empty_with_default_metadata() {
        let column = Column::new(ColumnType::Numeric);
        assert_eq!(column.column_type(), ColumnType::Numeric);
        assert_eq!(column.len(), 0);
        assert!(column.float_values().is_some());
        assert_eq!(column.meta().role, ColumnRole::Y);
    }

    #[test]
    fn get_reports_missing_for_nan_and_present_otherwise() {
        let column = Column::from_f64(&[1.0, f64::NAN, 3.0]);
        assert!(!column.get(0).is_missing());
        assert!(column.get(1).is_missing());
        assert!(column.is_missing(1));
    }

    #[test]
    fn reading_past_the_end_yields_missing_without_growing_length() {
        let column = Column::from_f64(&[1.0, 2.0]);
        assert!(column.get(99).is_missing());
        assert_eq!(column.len(), 2);
    }

    #[test]
    fn from_f64_copies_the_source_slice() {
        let mut source = vec![1.0, 2.0, 3.0];
        let column = Column::from_f64(&source);
        source[0] = 99.0;
        assert_eq!(column.get(0), Cell::Float(1.0));
    }

    #[test]
    fn text_columns_distinguish_empty_string_from_missing() {
        let column = Column::from_text(&[Some("a".to_string()), Some(String::new()), None]);
        assert_eq!(column.get(0), Cell::Text(Some("a".to_string())));
        assert_eq!(column.get(1), Cell::Text(Some(String::new())));
        assert!(column.get(1).is_missing());
        assert_eq!(column.get(2), Cell::Text(None));
        assert_eq!(column.get(9), Cell::Text(None));
        assert!(column.float_values().is_none());
    }

    #[test]
    fn from_block_rejects_mismatched_block_variant() {
        let err = Column::from_block(
            ColumnType::Numeric,
            ValueBlock::Mixed(vec![MixedCell::Empty]),
            ColumnMeta::default(),
        );
        assert!(matches!(err, Err(CoreError::TypeMismatch(_))));

        let err = Column::from_block(
            ColumnType::Mixed,
            ValueBlock::Float(vec![1.0]),
            ColumnMeta::default(),
        );
        assert!(matches!(err, Err(CoreError::TypeMismatch(_))));
    }

    #[test]
    fn set_values_overwrites_and_reports_delta_for_replay() {
        let mut column = Column::from_f64(&[1.0, 2.0, 3.0]);
        let delta = column
            .set_values(1, ValueBlock::Float(vec![20.0, 30.0]))
            .expect("valid write");
        assert_eq!(values_of(&column), vec![1.0, 20.0, 30.0]);
        assert_eq!(delta.start, 1);
        assert_eq!(delta.previous, ValueBlock::Float(vec![2.0, 3.0]));
        assert_eq!(delta.previous_length, 3);

        column
            .set_values(delta.start, delta.previous.clone())
            .expect("valid replay");
        column.set_length(delta.previous_length);
        assert_eq!(values_of(&column), vec![1.0, 2.0, 3.0]);
    }

    #[test]
    fn set_values_extends_and_fills_gap_with_missing() {
        let mut column = Column::from_f64(&[1.0]);
        let delta = column
            .set_values(3, ValueBlock::Float(vec![4.0]))
            .expect("valid write");
        assert_eq!(column.len(), 4);
        assert!(column.get(1).is_missing());
        assert!(column.get(2).is_missing());
        assert_eq!(column.get(3), Cell::Float(4.0));
        assert_eq!(delta.previous, ValueBlock::Float(Vec::new()));
        assert_eq!(delta.previous_length, 1);
    }

    #[test]
    fn set_values_rejects_mismatched_block_and_leaves_column_untouched() {
        let mut column = Column::from_f64(&[1.0, 2.0, 3.0]);
        let err = column.set_values(0, ValueBlock::Mixed(vec![MixedCell::Empty]));
        assert!(matches!(err, Err(CoreError::TypeMismatch(_))));
        assert_eq!(values_of(&column), vec![1.0, 2.0, 3.0]);
    }

    #[test]
    fn set_values_copies_on_write_when_a_reader_holds_the_arc() {
        let mut column = Column::from_f64(&[1.0, 2.0, 3.0]);

        // Simulates a live numpy view on the Python bridge side: refcount >= 2.
        let held = column.float_values().cloned().expect("numeric column");
        column
            .set_values(0, ValueBlock::Float(vec![9.0]))
            .expect("valid write");

        assert!(!Arc::ptr_eq(
            &held,
            column.float_values().expect("numeric column")
        ));
        assert_eq!(held.as_slice(), &[1.0, 2.0, 3.0]);
        assert_eq!(values_of(&column), vec![9.0, 2.0, 3.0]);
    }

    #[test]
    fn set_values_keeps_the_same_arc_without_a_concurrent_reader() {
        let mut column = Column::from_f64(&[1.0, 2.0, 3.0]);
        let before_ptr = Arc::as_ptr(column.float_values().expect("numeric column"));

        column
            .set_values(0, ValueBlock::Float(vec![9.0]))
            .expect("valid write");

        let after_ptr = Arc::as_ptr(column.float_values().expect("numeric column"));
        assert!(std::ptr::eq(before_ptr, after_ptr));
    }

    #[test]
    fn set_length_truncates_and_reports_removed_tail() {
        let mut column = Column::from_f64(&[1.0, 2.0, 3.0]);
        let delta = column.set_length(1);
        assert_eq!(values_of(&column), vec![1.0]);
        assert_eq!(delta.previous, ValueBlock::Float(vec![2.0, 3.0]));
        assert_eq!(delta.previous_length, 3);
    }

    #[test]
    fn set_length_extends_with_missing() {
        let mut column = Column::from_f64(&[1.0]);
        let delta = column.set_length(3);
        assert_eq!(column.len(), 3);
        assert!(column.get(1).is_missing());
        assert!(column.get(2).is_missing());
        assert_eq!(delta.previous, ValueBlock::Float(Vec::new()));
    }

    #[test]
    fn set_length_to_same_length_is_a_no_op() {
        let mut column = Column::from_f64(&[1.0, 2.0]);
        let delta = column.set_length(2);
        assert_eq!(delta.previous, ValueBlock::Float(Vec::new()));
        assert_eq!(column.len(), 2);
    }

    #[test]
    fn insert_then_delete_rows_round_trips() {
        let mut column = Column::from_f64(&[1.0, 2.0, 3.0]);
        column.insert_rows(1, 2).expect("valid insertion");
        assert_eq!(column.len(), 5);
        assert!(column.get(1).is_missing());
        assert!(column.get(2).is_missing());
        assert_eq!(column.get(3), Cell::Float(2.0));

        let removed = column.delete_rows(1, 2).expect("valid deletion");
        match removed {
            ValueBlock::Float(values) => {
                assert_eq!(values.len(), 2);
                assert!(values.iter().all(|value| value.is_nan()));
            }
            ValueBlock::Mixed(_) => panic!("expected a Float block"),
        }
        assert_eq!(values_of(&column), vec![1.0, 2.0, 3.0]);
    }

    #[test]
    fn delete_rows_clamps_to_end_of_column() {
        let mut column = Column::from_f64(&[1.0, 2.0]);
        let removed = column.delete_rows(1, 99).expect("valid deletion");
        assert_eq!(removed, ValueBlock::Float(vec![2.0]));
        assert_eq!(values_of(&column), vec![1.0]);
    }

    #[test]
    fn insert_rows_rejects_out_of_bounds_but_accepts_end_position() {
        let mut column = Column::from_f64(&[1.0]);
        assert!(matches!(
            column.insert_rows(5, 1),
            Err(CoreError::IndexOutOfBounds(_))
        ));
        column
            .insert_rows(1, 1)
            .expect("valid insertion at the end of the column");
        assert_eq!(column.len(), 2);
    }

    #[test]
    fn delete_rows_of_zero_count_is_a_no_op() {
        let mut column = Column::from_f64(&[1.0, 2.0]);
        let removed = column.delete_rows(0, 0).expect("valid deletion");
        assert_eq!(removed, ValueBlock::Float(Vec::new()));
        assert_eq!(values_of(&column), vec![1.0, 2.0]);
    }

    #[test]
    fn text_column_insert_and_delete_rows() {
        let mut column = Column::from_text(&[Some("a".to_string()), Some("b".to_string())]);
        column.insert_rows(1, 1).expect("valid insertion");
        assert_eq!(column.get(1), Cell::Text(None));

        let removed = column.delete_rows(0, 1).expect("valid deletion");
        assert_eq!(
            removed,
            ValueBlock::Mixed(vec![MixedCell::Text("a".to_string())])
        );
    }

    #[test]
    fn set_meta_replaces_and_returns_the_previous_metadata() {
        let mut column = Column::new(ColumnType::Numeric);
        let new_meta = ColumnMeta {
            role: ColumnRole::X,
            unit: "ms".to_string(),
            ..ColumnMeta::default()
        };
        let previous = column.set_meta(new_meta.clone());
        assert_eq!(previous.unit, "");
        assert_eq!(previous.role, ColumnRole::Y);
        assert_eq!(column.meta(), &new_meta);
    }

    // `set_column_type_text_to_numeric_parses_values` was removed: it
    // tested `Text → Numeric` through `Column::from_text`, which now
    // builds a `Mixed` column directly — the same conversion is
    // covered by `mixed_to_numeric_parses_text_cells_like_a_text_column_would`
    // below.

    #[test]
    fn mixed_cell_into_text_formats_numbers_with_the_display_form() {
        // Former `set_column_type_numeric_to_text_formats_values`: there is no
        // longer a `ColumnType::Text` to convert to, so the same
        // coverage (display format of a number, `NaN` → `None`) applies
        // directly to `MixedCell::into_text`.
        assert_eq!(MixedCell::from_f64(1.0).into_text(), Some("1".to_string()));
        assert_eq!(MixedCell::from_f64(f64::NAN).into_text(), None);
    }

    #[test]
    fn set_column_type_to_same_type_is_a_no_op_snapshot() {
        let mut column = Column::from_f64(&[1.0]);
        let (old_type, old_values) = column.set_column_type(ColumnType::Numeric);
        assert_eq!(old_type, ColumnType::Numeric);
        assert_eq!(old_values, ColumnValues::Float(Arc::new(vec![1.0])));
        assert_eq!(values_of(&column), vec![1.0]);
    }

    #[test]
    fn set_column_type_numeric_to_datetime_keeps_values() {
        let mut column = Column::from_f64(&[1.0, 2.0]);
        column.set_column_type(ColumnType::DateTime);
        assert_eq!(column.column_type(), ColumnType::DateTime);
        assert_eq!(values_of(&column), vec![1.0, 2.0]);
        assert!(column.float_values().is_some());
    }

    #[test]
    fn restore_undoes_set_column_type_exactly() {
        let original = Column::from_f64(&[1.0, 2.0]);
        let mut column = original.clone();
        let (old_type, old_values) = column.set_column_type(ColumnType::Mixed);
        column
            .restore(old_type, old_values)
            .expect("valid restoration");
        assert_eq!(column, original);

        let err = column.restore(
            ColumnType::Numeric,
            ColumnValues::Mixed(Arc::new(vec![MixedCell::Empty])),
        );
        assert!(matches!(err, Err(CoreError::TypeMismatch(_))));
    }

    #[test]
    fn float_equality_is_bit_for_bit_not_ieee() {
        // Two identical columns, `NaN` included, are equal.
        assert_eq!(
            Column::from_f64(&[1.0, f64::NAN]),
            Column::from_f64(&[1.0, f64::NAN])
        );
        assert_ne!(
            Column::from_f64(&[1.0, f64::NAN]),
            Column::from_f64(&[1.0, 2.0])
        );

        assert_eq!(Cell::Float(f64::NAN), Cell::Float(f64::NAN));
        assert_eq!(
            ValueBlock::Float(vec![f64::NAN]),
            ValueBlock::Float(vec![f64::NAN])
        );
        assert_eq!(
            ColumnValues::Float(Arc::new(vec![f64::NAN])),
            ColumnValues::Float(Arc::new(vec![f64::NAN]))
        );

        // Distinct bit pattern: `0.0` and `-0.0` stay different.
        assert_ne!(Cell::Float(0.0), Cell::Float(-0.0));

        // Text types keep a normal comparison, unaffected.
        assert_eq!(
            Cell::Text(Some("a".to_string())),
            Cell::Text(Some("a".to_string()))
        );
        assert_ne!(Cell::Text(None), Cell::Text(Some(String::new())));
    }

    #[test]
    fn version_starts_at_zero_and_advances_on_every_data_mutator() {
        let mut column = Column::from_f64(&[1.0, 2.0, 3.0]);
        assert_eq!(column.version(), 0);

        column.set_values(0, ValueBlock::Float(vec![9.0])).unwrap();
        assert_eq!(column.version(), 1);
        column.set_length(2);
        assert_eq!(column.version(), 2);
        column.insert_rows(0, 1).unwrap();
        assert_eq!(column.version(), 3);
        column.delete_rows(0, 1).unwrap();
        assert_eq!(column.version(), 4);

        let (old_type, old_values) = column.set_column_type(ColumnType::Mixed);
        assert_eq!(column.version(), 5);
        column.restore(old_type, old_values).unwrap();
        assert_eq!(column.version(), 6);
    }

    #[test]
    fn version_advances_even_when_the_call_turns_out_to_be_a_no_op() {
        // The direction of the approximation is deliberate: a false "has changed"
        // costs a recalculation, a false "has not changed" would show a
        // wrong result.
        let mut column = Column::from_f64(&[1.0, 2.0]);
        column.insert_rows(0, 0).unwrap();
        assert_eq!(column.version(), 1);
        column.set_length(2);
        assert_eq!(column.version(), 2);
        column.delete_rows(0, 0).unwrap();
        assert_eq!(column.version(), 3);
        assert_eq!(values_of(&column), vec![1.0, 2.0]);
    }

    #[test]
    fn version_ignores_metadata_and_refused_calls() {
        let mut column = Column::from_f64(&[1.0, 2.0]);

        column.set_meta(ColumnMeta {
            unit: "ms".to_string(),
            ..ColumnMeta::default()
        });
        assert_eq!(column.version(), 0, "a unit is not data");

        assert!(column
            .set_values(0, ValueBlock::Mixed(vec![MixedCell::Empty]))
            .is_err());
        assert_eq!(column.version(), 0, "a refused call writes nothing");

        assert!(column.insert_rows(99, 1).is_err());
        assert_eq!(column.version(), 0, "an out-of-bounds call writes nothing");

        assert!(column
            .restore(
                ColumnType::Numeric,
                ColumnValues::Mixed(Arc::new(vec![MixedCell::Empty]))
            )
            .is_err());
        assert_eq!(column.version(), 0);
    }

    #[test]
    fn equality_ignores_the_version_counter() {
        // This is what lets the property "after undo, the tree is bit for
        // bit the one from before" hold: undoing is one more write.
        let original = Column::from_f64(&[1.0, 2.0]);
        let mut column = original.clone();

        let delta = column
            .set_values(0, ValueBlock::Float(vec![9.0]))
            .expect("valid write");
        column.set_values(delta.start, delta.previous).unwrap();
        column.set_length(delta.previous_length);

        assert_eq!(column.version(), 3);
        assert_eq!(original.version(), 0);
        assert_eq!(column, original, "same data, different counters");
    }

    #[test]
    fn from_block_with_version_restores_an_imposed_counter() {
        let column = Column::from_block_with_version(
            ColumnType::Numeric,
            ValueBlock::Float(vec![1.0]),
            ColumnMeta::default(),
            47,
        )
        .expect("valid block");
        assert_eq!(column.version(), 47);
        assert_eq!(
            Column::from_block(
                ColumnType::Numeric,
                ValueBlock::Float(vec![1.0]),
                ColumnMeta::default()
            )
            .unwrap()
            .version(),
            0
        );
    }

    // -- mixed-type columns ----------------------------------------

    fn mixed_cells(column: &Column) -> Vec<MixedCell> {
        (0..column.len()).map(|row| column.cell(row)).collect()
    }

    #[test]
    fn a_mixed_column_holds_a_different_type_on_every_row() {
        let column = Column::from_mixed(&[
            MixedCell::Number(1.2),
            MixedCell::Text("n/a".to_string()),
            MixedCell::Empty,
            MixedCell::Number(1.5),
        ]);
        assert_eq!(column.column_type(), ColumnType::Mixed);
        assert_eq!(column.len(), 4);
        assert_eq!(column.cell(0), MixedCell::Number(1.2));
        assert_eq!(column.cell(1), MixedCell::Text("n/a".to_string()));
        assert_eq!(column.cell(2), MixedCell::Empty);
        // Out of bounds: empty, never an error.
        assert_eq!(column.cell(99), MixedCell::Empty);
    }

    #[test]
    fn a_mixed_column_has_no_contiguous_float_buffer_but_a_numeric_view() {
        let column = Column::from_mixed(&[
            MixedCell::Number(1.2),
            MixedCell::Text("n/a".to_string()),
            MixedCell::Empty,
        ]);
        assert!(
            column.float_values().is_none(),
            "no contiguous f64 buffer, hence no numpy view without copying"
        );
        let view = column
            .numeric_view()
            .expect("a mixed column can be plotted");
        assert_eq!(view[0], 1.2);
        assert!(view[1].is_nan(), "text is not plotted");
        assert!(view[2].is_nan(), "the empty cell is not plotted");
    }

    #[test]
    fn numeric_view_is_zero_copy_for_homogeneous_columns_only() {
        let numeric = Column::from_f64(&[1.0, 2.0]);
        let shared = numeric.numeric_view().expect("plottable numeric column");
        assert!(
            Arc::ptr_eq(&shared, numeric.float_values().expect("f64 buffer")),
            "no copy for a numeric column"
        );
        let text_only = Column::from_text(&[Some("a".to_string())]);
        let view = text_only
            .numeric_view()
            .expect("a mixed column stays plottable (new copy)");
        assert!(
            view[0].is_nan(),
            "a text cell plots nothing, but the column is not refused"
        );
        assert!(
            text_only.float_values().is_none(),
            "no contiguous f64 buffer for a mixed column"
        );
    }

    #[test]
    fn mixed_cells_distinguish_empty_from_an_empty_string_and_from_nan() {
        let column = Column::from_mixed(&[
            MixedCell::Empty,
            MixedCell::Text(String::new()),
            MixedCell::Number(f64::NAN),
        ]);
        // Stored distinctly...
        assert_ne!(column.cell(0), column.cell(1));
        assert_ne!(column.cell(0), column.cell(2));
        // ... but all three "missing" on display.
        assert!((0..3).all(|row| column.cell(row).is_missing()));
        assert!((0..3).all(|row| column.is_missing(row)));
    }

    #[test]
    fn writing_a_mixed_block_reports_a_replayable_delta() {
        let mut column = Column::from_mixed(&[
            MixedCell::Number(1.0),
            MixedCell::Number(2.0),
            MixedCell::Number(3.0),
        ]);
        let delta = column
            .set_values(
                1,
                ValueBlock::Mixed(vec![MixedCell::Text("oops".to_string()), MixedCell::Empty]),
            )
            .expect("valid write");
        assert_eq!(
            mixed_cells(&column),
            vec![
                MixedCell::Number(1.0),
                MixedCell::Text("oops".to_string()),
                MixedCell::Empty,
            ]
        );
        assert_eq!(
            delta.previous,
            ValueBlock::Mixed(vec![MixedCell::Number(2.0), MixedCell::Number(3.0)])
        );

        column
            .set_values(delta.start, delta.previous.clone())
            .expect("valid replay");
        column.set_length(delta.previous_length);
        assert_eq!(
            mixed_cells(&column),
            vec![
                MixedCell::Number(1.0),
                MixedCell::Number(2.0),
                MixedCell::Number(3.0),
            ]
        );
    }

    #[test]
    fn a_mixed_column_refuses_a_float_block() {
        let mut column = Column::from_mixed(&[MixedCell::Number(1.0)]);
        assert!(matches!(
            column.set_values(0, ValueBlock::Float(vec![2.0])),
            Err(CoreError::TypeMismatch(_))
        ));
        assert_eq!(column.version(), 0, "a refused call writes nothing");
    }

    #[test]
    fn mixed_rows_insert_and_delete_with_empty_as_the_missing_value() {
        let mut column =
            Column::from_mixed(&[MixedCell::Number(1.0), MixedCell::Text("a".to_string())]);
        column.insert_rows(1, 1).expect("valid insertion");
        assert_eq!(column.cell(1), MixedCell::Empty);
        assert_eq!(column.len(), 3);

        let removed = column.delete_rows(0, 2).expect("valid deletion");
        assert_eq!(
            removed,
            ValueBlock::Mixed(vec![MixedCell::Number(1.0), MixedCell::Empty])
        );
        assert_eq!(mixed_cells(&column), vec![MixedCell::Text("a".to_string())]);
    }

    #[test]
    fn set_length_extends_a_mixed_column_with_empty_cells() {
        let mut column = Column::from_mixed(&[MixedCell::Number(1.0)]);
        let delta = column.set_length(3);
        assert_eq!(column.len(), 3);
        assert_eq!(column.cell(2), MixedCell::Empty);
        assert_eq!(delta.previous, ValueBlock::Mixed(Vec::new()));

        let delta = column.set_length(1);
        assert_eq!(
            delta.previous,
            ValueBlock::Mixed(vec![MixedCell::Empty, MixedCell::Empty])
        );
    }

    #[test]
    fn mixed_cell_from_text_promotes_what_parses_and_keeps_the_rest() {
        // Former `text_to_mixed_promotes_what_parses_and_keeps_the_rest`: the
        // promotion "typed text → mixed cell" no longer has a homogeneous
        // `Text` type upstream (`Column::from_text` builds `Mixed` directly,
        // cell by cell, without reparsing — see its doc), so
        // the coverage is directly on `MixedCell::from_text`, the
        // real entry point of this promotion (typing in a mixed
        // column, `stokesforge-app::state`).
        assert_eq!(
            MixedCell::from_text(Some("1.2".to_string())),
            MixedCell::Number(1.2)
        );
        assert_eq!(
            MixedCell::from_text(Some("n/a".to_string())),
            MixedCell::Text("n/a".to_string())
        );
        assert_eq!(MixedCell::from_text(None), MixedCell::Empty);
        assert_eq!(
            MixedCell::from_text(Some(String::new())),
            MixedCell::Text(String::new()),
            "a deliberate empty string stays a text, not an empty cell"
        );
    }

    #[test]
    fn numeric_to_mixed_is_lossless_and_maps_nan_to_empty() {
        let mut column = Column::from_f64(&[1.0, f64::NAN, -0.0]);
        column.set_column_type(ColumnType::Mixed);
        assert_eq!(column.cell(0), MixedCell::Number(1.0));
        assert_eq!(column.cell(1), MixedCell::Empty);
        assert_eq!(column.cell(2), MixedCell::Number(-0.0));

        // And back: the missing value makes a `NaN` again.
        column.set_column_type(ColumnType::Numeric);
        assert_eq!(column.get(0), Cell::Float(1.0));
        assert!(column.get(1).is_missing());
        assert_eq!(column.get(2), Cell::Float(-0.0));
    }

    #[test]
    fn mixed_to_numeric_parses_text_cells_like_a_text_column_would() {
        let mut column = Column::from_mixed(&[
            MixedCell::Number(1.5),
            MixedCell::Text("3".to_string()),
            MixedCell::Text("abc".to_string()),
            MixedCell::Empty,
        ]);
        column.set_column_type(ColumnType::Numeric);
        assert_eq!(column.column_type(), ColumnType::Numeric);
        assert_eq!(column.get(0), Cell::Float(1.5));
        assert_eq!(column.get(1), Cell::Float(3.0));
        assert!(column.get(2).is_missing());
        assert!(column.get(3).is_missing());
    }

    #[test]
    fn mixed_cell_into_text_reads_each_cell_kind() {
        // Former `mixed_to_text_writes_numbers_with_the_display_form`, which
        // went through `set_column_type(ColumnType::Text)` — no more `Text`
        // target for a column type, so the same coverage applies
        // directly to `MixedCell::into_text`.
        assert_eq!(MixedCell::Number(1.0).into_text(), Some("1".to_string()));
        assert_eq!(
            MixedCell::Text("n/a".to_string()).into_text(),
            Some("n/a".to_string())
        );
        assert_eq!(MixedCell::Empty.into_text(), None);
    }

    #[test]
    fn restore_undoes_a_conversion_from_mixed_to_numeric_exactly() {
        let original =
            Column::from_mixed(&[MixedCell::Number(1.2), MixedCell::Text("n/a".to_string())]);
        let mut column = original.clone();
        let (old_type, old_values) = column.set_column_type(ColumnType::Numeric);
        assert_ne!(column, original);
        column
            .restore(old_type, old_values)
            .expect("valid restoration");
        assert_eq!(column, original);

        // And the symmetric refusal: a type that does not match the values.
        assert!(matches!(
            column.restore(ColumnType::Mixed, ColumnValues::Float(Arc::new(vec![0.0]))),
            Err(CoreError::TypeMismatch(_))
        ));
    }

    #[test]
    fn a_block_knows_which_column_type_it_fits() {
        assert!(ValueBlock::Float(vec![1.0]).fits(ColumnType::Numeric));
        assert!(ValueBlock::Float(vec![1.0]).fits(ColumnType::DateTime));
        assert!(!ValueBlock::Float(vec![1.0]).fits(ColumnType::Mixed));
        assert!(ValueBlock::Mixed(vec![MixedCell::Empty]).fits(ColumnType::Mixed));
        assert!(!ValueBlock::Mixed(vec![MixedCell::Empty]).fits(ColumnType::Numeric));
    }

    #[test]
    fn into_mixed_follows_the_same_rules_as_the_type_conversion() {
        assert_eq!(
            ValueBlock::Float(vec![1.0, f64::NAN]).into_mixed(),
            vec![MixedCell::Number(1.0), MixedCell::Empty]
        );
        // An already-mixed block passes through as is, without reparsing: a numeric
        // text deliberately stored as text stays so.
        assert_eq!(
            ValueBlock::Mixed(vec![MixedCell::Text("2".to_string())]).into_mixed(),
            vec![MixedCell::Text("2".to_string())]
        );
    }

    #[test]
    fn a_mixed_cell_never_reparses_the_text_it_stores() {
        assert!(MixedCell::Text("42".to_string()).as_number().is_nan());
        assert_eq!(MixedCell::Number(42.0).as_number(), 42.0);
        assert!(MixedCell::Empty.as_number().is_nan());
    }

    #[test]
    fn reading_a_mixed_cell_through_the_common_cell_vocabulary() {
        let column = Column::from_mixed(&[
            MixedCell::Number(1.5),
            MixedCell::Text("a".to_string()),
            MixedCell::Empty,
        ]);
        assert_eq!(column.get(0), Cell::Float(1.5));
        assert_eq!(column.get(1), Cell::Text(Some("a".to_string())));
        assert_eq!(
            column.get(2),
            Cell::Text(None),
            "an empty cell reads as an absent value"
        );
    }

    #[test]
    fn number_at_reads_without_allocating_for_every_column_type() {
        let mixed = Column::from_mixed(&[MixedCell::Number(7.0), MixedCell::Text("x".to_string())]);
        assert_eq!(mixed.values().number_at(0), 7.0);
        assert!(mixed.values().number_at(1).is_nan());
        assert!(mixed.values().number_at(99).is_nan());
        assert_eq!(Column::from_f64(&[3.0]).values().number_at(0), 3.0);
        assert!(Column::from_text(&[Some("3".to_string())])
            .values()
            .number_at(0)
            .is_nan());
    }

    #[test]
    fn value_block_helpers_build_missing_and_empty_blocks() {
        match ValueBlock::missing_for(ColumnType::Numeric, 2) {
            ValueBlock::Float(values) => {
                assert_eq!(values.len(), 2);
                assert!(values.iter().all(|value| value.is_nan()));
            }
            ValueBlock::Mixed(_) => panic!("expected a Float block"),
        }
        assert_eq!(
            ValueBlock::missing_for(ColumnType::Mixed, 2),
            ValueBlock::Mixed(vec![MixedCell::Empty, MixedCell::Empty])
        );
        assert!(ValueBlock::empty_for(ColumnType::Numeric).is_empty());
        assert!(ValueBlock::empty_for(ColumnType::Mixed).is_empty());
    }

    // --- ValueBlock::from_texts (#258): a single rule for typing and pasting.

    const MIXED_TEXTS: [&str; 12] = [
        "1",
        "1,5",
        "",
        "  ",
        "n/a",
        "2026-10-15",
        "10/10/2026 14:30",
        "86400",
        "inf",
        "-Infinity",
        "nan",
        "1e400",
    ];

    fn day(text: &str) -> f64 {
        crate::datetime::parse_datetime(text).expect("readable date")
    }

    #[test]
    fn from_texts_on_a_whole_column_equals_cell_by_cell_for_the_three_types() {
        for kind in [ColumnType::Numeric, ColumnType::DateTime, ColumnType::Mixed] {
            let (column, promoting_column) = ValueBlock::from_texts(kind, &MIXED_TEXTS);
            let mut cell_by_cell = Vec::new();
            let mut any_promoting = false;
            for text in MIXED_TEXTS {
                let (block, promoting) = ValueBlock::from_texts(kind, &[text]);
                any_promoting |= promoting;
                cell_by_cell.extend(block.into_mixed());
            }
            assert_eq!(
                column.into_mixed(),
                cell_by_cell,
                "column and cells diverge for {kind:?}"
            );
            assert_eq!(promoting_column, any_promoting, "promoting for {kind:?}");
        }
    }

    /// #311 (E1): a bare "2026", typed in a Date/Time column, stays a
    /// number of POSIX seconds (1970-01-01 00:33:46), not a year.
    #[test]
    fn from_texts_datetime_reads_a_bare_number_as_seconds() {
        let (block, promoting) = ValueBlock::from_texts(ColumnType::DateTime, &["2026"]);
        assert_eq!(block, ValueBlock::Float(vec![2026.0]));
        assert!(!promoting);
    }

    #[test]
    fn from_texts_datetime_keeps_the_date_as_a_value_in_a_widened_block() {
        let (block, promoting) =
            ValueBlock::from_texts(ColumnType::DateTime, &["2026-10-15", "n/a"]);
        assert_eq!(
            block,
            ValueBlock::Mixed(vec![
                MixedCell::Number(day("2026-10-15")),
                MixedCell::Text("n/a".to_string()),
            ])
        );
        assert!(promoting);
    }

    #[test]
    fn from_texts_datetime_without_stray_text_stays_a_float_block() {
        let (block, promoting) =
            ValueBlock::from_texts(ColumnType::DateTime, &["2026-10-15", " 86400 ", ""]);
        match block {
            ValueBlock::Float(values) => {
                assert_eq!(values[0], day("2026-10-15"));
                assert_eq!(values[1], 86400.0);
                assert!(values[2].is_nan());
            }
            ValueBlock::Mixed(_) => panic!("expected a Float block"),
        }
        assert!(!promoting);
    }

    #[test]
    fn from_texts_numeric_does_not_read_dates() {
        let (block, promoting) = ValueBlock::from_texts(ColumnType::Numeric, &["2026-10-15"]);
        assert_eq!(
            block,
            ValueBlock::Mixed(vec![MixedCell::Text("2026-10-15".to_string())])
        );
        assert!(promoting);
    }

    #[test]
    fn from_texts_mixed_reads_neither_infinity_nor_nan() {
        let (block, promoting) = ValueBlock::from_texts(ColumnType::Mixed, &["inf", "nan", "-inf"]);
        assert_eq!(
            block,
            ValueBlock::Mixed(vec![
                MixedCell::Text("inf".to_string()),
                MixedCell::Text("nan".to_string()),
                MixedCell::Text("-inf".to_string()),
            ])
        );
        assert!(!promoting);
    }

    #[test]
    fn from_texts_numeric_keeps_infinity_and_overflow_as_text() {
        for text in ["inf", "1e400"] {
            let (block, promoting) = ValueBlock::from_texts(ColumnType::Numeric, &[text]);
            assert_eq!(
                block,
                ValueBlock::Mixed(vec![MixedCell::Text(text.to_string())]),
                "{text}"
            );
            assert!(promoting, "{text}");
        }
    }

    #[test]
    fn from_texts_empty_cell_is_a_missing_value_not_a_text() {
        let (block, promoting) = ValueBlock::from_texts(ColumnType::Numeric, &["1", ""]);
        assert_eq!(block, ValueBlock::Float(vec![1.0, f64::NAN]));
        assert!(!promoting);
        let (block, promoting) = ValueBlock::from_texts(ColumnType::Mixed, &["", "  ", "x"]);
        assert_eq!(
            block,
            ValueBlock::Mixed(vec![
                MixedCell::Empty,
                MixedCell::Empty,
                MixedCell::Text("x".to_string())
            ])
        );
        assert!(!promoting);
    }

    #[test]
    fn from_texts_trims_kept_text_and_reads_the_decimal_comma() {
        let (block, _) = ValueBlock::from_texts(ColumnType::Mixed, &["  1,5 ", "  a b  "]);
        assert_eq!(
            block,
            ValueBlock::Mixed(vec![
                MixedCell::Number(1.5),
                MixedCell::Text("a b".to_string())
            ])
        );
    }
}

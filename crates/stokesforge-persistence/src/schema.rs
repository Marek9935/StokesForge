//! Transport structures ("on-disk schema"), private to the crate.
//!
//! This module does **not** know `stokesforge-core` the other way round: it is
//! `convert.rs` that knows how to build a `NodeRecord` from a `Node` and
//! the reverse, never the other way. The schema is deliberately
//! independent of the in-memory model (see the crate doc): it evolves according to the
//! version rule of `docs/persistence.md` (additive `#[serde(default)]`
//! fields; removal or rename = new version, with
//! migration) without ever deriving `Serialize` on
//! the types of `stokesforge-core`.
//!
//! All these structures are serialized **as named maps**
//! (`rmp_serde::to_vec_named`), which makes the file self-describing: a
//! field missing at read takes its default value (`#[serde(default)]`,
//! to be added on every field created after version 1) and a field unknown at
//! read is silently ignored — this is the mechanism that will make it possible
//! to add data (for example a graph specification) without
//! breaking the files already written.
//!
//! Until now this mechanism only covered the **fields** — not the
//! **variants** of the externally-tagged enums (`KindRecord`, `PlotTypeRecord`,
//! …): an unknown variant made the whole enum that carries it fail, hence
//! the whole node, hence the whole file (C5, level 2,
//! `guide-de-correction-robustesse.md` §F3). `KindRecord` and `PlotTypeRecord`
//! now write `Serialize`/`Deserialize` by hand (see
//! `unknown.rs`) to capture an unknown variant in an
//! `Unknown` variant rather than propagating the failure.

use serde::{Deserialize, Serialize};
use stokesforge_plot::RawVariant;

use crate::recipe_record::ImportSourceRecord;
use crate::unknown::{deserialize_with_fallback, serialize_raw_variant};

/// File signature: the first four bytes, even before the version
/// number.
pub(crate) const MAGIC: &[u8; 4] = b"STKF";

/// Version number of the format that this reader/writer can produce and
/// understand. A file whose version number is higher is
/// refused without its body being read (`PersistenceError::UnsupportedVersion`).
pub(crate) const FORMAT_VERSION: u32 = 1;

/// Body of the file (after the magic header + version): a complete
/// `Project`, flat.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct FileBody {
    /// Identifier of the root node.
    pub(crate) root: u64,
    /// Persisted value of the identifier counter (`Project::next_id_hint`).
    pub(crate) next_id_hint: u64,
    /// All the nodes of the tree, in pre-order from the root — this
    /// order is what makes two saves of the same project identical
    /// byte for byte.
    pub(crate) nodes: Vec<NodeRecord>,
    /// Colors saved in the project, by usage
    /// (`stokesforge_plot::SavedColors`, carried by the tree). Absent from
    /// files written before they were added: `#[serde(default)]` brings them back to
    /// empty lists. Placed before `window_geometries` and `save_counter`,
    /// for the same reason as theirs (see below).
    #[serde(default)]
    pub(crate) saved_colors: SavedColorsRecord,
    /// Analysis calculations (#181). Absent from earlier files: empty
    /// list. Placed before `window_geometries` and `save_counter`, for the same
    /// reason as `saved_colors`.
    #[serde(default)]
    pub(crate) calculations: Vec<crate::calculations::CalculationRecord>,
    #[serde(default)]
    pub(crate) correction_traces: Vec<crate::calculations::CorrectionTraceRecord>,
    /// Linked transposed sheets (#253). Absent from earlier files:
    /// empty list. Placed before `window_geometries` and `save_counter`, for the
    /// same reason as `calculations`.
    #[serde(default)]
    pub(crate) transposes: Vec<crate::transposes::TransposeRecord>,
    /// Geometry of the windows (position/size), by node identifier —
    /// block **separate from the undoable tree** (see the head doc of
    /// `stokesforge-session`: the geometry does not live in `Project`, never goes
    /// through a command, and never appears in `nodes` above).
    /// Absent from files written before this block was added:
    /// `#[serde(default)]` then brings it back to an empty table, which is the
    /// intended behavior — an old file described no window position,
    /// there is nothing to recover. The keys are raw `NodeId`s,
    /// as everywhere else in this schema (`root`, `parent`, …); a key
    /// that matches no node of `nodes` (corrupted file, or node
    /// deleted by a third-party tool without purging its geometry) is silently
    /// filtered at load rather than making the whole
    /// file fail (invariant 10: a data defect degrades, it does not
    /// crash) — see `load_with_geometries`. Placed **before** `save_counter`
    /// below: `tests/roundtrip.rs` (`saving_over_an_existing_valid_file_replaces_it_atomically`)
    /// relies on only the last byte of the file changing between two
    /// successive saves, which assumes that `save_counter` stays the
    /// very last serialized field.
    #[serde(default)]
    pub(crate) window_geometries: std::collections::BTreeMap<u64, WindowGeometryRecord>,
    /// Width of the columns of a sheet (#325), in logical points, by
    /// column identifier — same status as `window_geometries`: block
    /// **outside the undoable tree**, absent from earlier files
    /// (`#[serde(default)]`: empty table, the columns take the default
    /// width), raw `NodeId` keys bounded by `check_node_id`. At read,
    /// a key without a node and a value that is not a strictly positive
    /// finite real are dropped one by one (invariant 10). Placed **before**
    /// `save_counter`, which stays the last serialized field (see above).
    #[serde(default)]
    pub(crate) column_widths: std::collections::BTreeMap<u64, f32>,
    /// Save counter of the file (B8,
    /// `guide-de-correction-robustesse.md` §F2): incremented at each successful
    /// `save`. This is what lets `save` detect that another writer
    /// modified the file since the last load/save of this
    /// process, without confusing two distinct instances loaded from the same
    /// file. Absent from files written before 2026-09-11:
    /// `#[serde(default)]` then brings it back to 0, which is the intended
    /// behavior — a file without a counter has no reader anyway that
    /// retains one.
    #[serde(default)]
    pub(crate) save_counter: u64,
}

/// [`stokesforge_plot::SavedColors`], flat. Each list is bounded at
/// read (`SavedColors::bounded`), a file not being bounded.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub(crate) struct SavedColorsRecord {
    #[serde(default)]
    pub(crate) curves: Vec<String>,
    #[serde(default)]
    pub(crate) text: Vec<String>,
    #[serde(default)]
    pub(crate) backgrounds: Vec<String>,
    /// Saved color gradients (#178). Absent from earlier files.
    #[serde(default)]
    pub(crate) gradients: Vec<SavedGradientRecord>,
}

/// [`stokesforge_plot::SavedGradient`], flat.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct SavedGradientRecord {
    pub(crate) name: String,
    pub(crate) stops: Vec<String>,
}

/// A node of the tree, flat (membership in the children of a parent is
/// rebuilt by `Tree::from_nodes`, not carried twice here — except for
/// containers, where the order of the children must be persisted explicitly).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct NodeRecord {
    /// Identifier of the node.
    pub(crate) id: u64,
    /// Short name.
    pub(crate) name: String,
    /// Identifier of the parent, `None` for the root.
    pub(crate) parent: Option<u64>,
    /// Kind and own content.
    pub(crate) kind: KindRecord,
}

/// Kind of a node, flat. "Externally tagged" enum (the
/// default behavior of `serde`): each value serializes as
/// a single-key map (the name of the variant) whose value is the
/// content — this is what keeps the file self-describing down to its
/// enums.
///
/// `Serialize`/`Deserialize` are written by hand, not derived (C5, level
/// 2, `guide-de-correction-robustesse.md` §F3 — see `unknown.rs` for the
/// mechanism): a variant that this reader does not know becomes
/// [`KindRecord::Unknown`] instead of making the whole file fail.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum KindRecord {
    /// Folder.
    Folder {
        /// Ordered children.
        children: Vec<u64>,
    },
    /// Workbook.
    Workbook {
        /// Sheets, in tab order.
        sheets: Vec<u64>,
        hidden: bool,
        /// Window label.
        label: String,
        /// Window comment.
        comment: String,
    },
    /// Worksheet.
    Worksheet {
        /// Columns, from left to right.
        columns: Vec<u64>,
    },
    /// Data column.
    Column {
        /// Data type.
        column_type: TypeRecord,
        /// Plot role.
        role: RoleRecord,
        /// Displayed unit.
        unit: String,
        /// Long name / description.
        long_name: String,
        /// Comment.
        comment: String,
        /// Display format.
        display_format: String,
        /// Values of the column.
        values: ValuesRecord,
        /// Modification counter of the data (`Column::version`). Absent
        /// from files written before 2026-09-10: `KindRecordKnown` (further
        /// down) brings this field back to 0 via `#[serde(default)]`, which is the
        /// intended behavior — a file without a counter has no reader anyway
        /// that retains one.
        version: u64,
        /// Calculation formula (`ColumnMeta::formula`), `None` for an ordinary
        /// column: text fragments and **identifiers** of the columns read
        /// (#321), never position letters. Field `formula_parts`,
        /// absent from earlier files: `KindRecordKnown` brings it back to
        /// `None` via `#[serde(default)]`. The old `formula` field (text
        /// in letters, from before #321) is no longer read — the format does not have to stay
        /// compatible during development: the column loads with its values
        /// and without a formula. The values present in `values` above are
        /// treated as a cache: `stokesforge-session::actor` triggers a
        /// recalculation after every load (see its head doc), so a
        /// disagreement between a file written by a third party and what the formula
        /// would recalculate does not survive the first `load`.
        formula_parts: Option<Vec<FormulaPartRecord>>,
        /// Metadata of the original file (`ColumnMeta::file_metadata`),
        /// key / value pairs in order. Absent from files written
        /// before this field: `KindRecordKnown` brings them back to an empty list.
        file_metadata: Vec<(String, String)>,
        /// Import source (`ColumnMeta::import_source`): path, recipe, column
        /// and block index. Absent from earlier files.
        import_source: Option<Box<ImportSourceRecord>>,
        /// Value of the special x' row (`ColumnMeta::x_prime`, #164).
        /// Absent from earlier files: `None`.
        x_prime: Option<f64>,
    },
    /// Graph.
    Graph {
        hidden: bool,
        /// Window label.
        label: String,
        /// Window comment.
        comment: String,
        /// Description of the graph (`PlotSpec`). Absent from files written
        /// before phase R3: `KindRecordKnown` brings this field back to
        /// [`PlotSpecRecord::default`] via `#[serde(default)]`, which converts
        /// to the `PlotSpec::default()` of `stokesforge-plot` (one panel,
        /// no series) — exactly what a file without a spec already described
        /// in memory before this phase.
        spec: PlotSpecRecord,
    },
    /// Matrix.
    Matrix {
        hidden: bool,
        /// Window label.
        label: String,
        /// Window comment.
        comment: String,
    },
    /// Note.
    Note {
        hidden: bool,
        /// Window label.
        label: String,
        /// Window comment.
        comment: String,
        /// Text of the note.
        text: String,
    },
    /// Kind of node that none of the variants above names: a
    /// newer version of StokesForge wrote this file. Raw name and content,
    /// kept for a faithful rewrite.
    Unknown(RawVariant),
}

/// Strict mirror of [`KindRecord`], without [`KindRecord::Unknown`]: it is the
/// schema that a file written by this reader can produce. `unknown.rs`
/// uses it to *try* the strict decoding before falling back on
/// [`KindRecord::Unknown`] — see the head doc of this module.
#[derive(Debug, Clone, PartialEq, Deserialize)]
enum KindRecordKnown {
    Folder {
        children: Vec<u64>,
    },
    Workbook {
        sheets: Vec<u64>,
        #[serde(default)]
        hidden: bool,
        label: String,
        comment: String,
    },
    Worksheet {
        columns: Vec<u64>,
    },
    Column {
        column_type: TypeRecord,
        role: RoleRecord,
        unit: String,
        long_name: String,
        comment: String,
        display_format: String,
        values: ValuesRecord,
        #[serde(default)]
        version: u64,
        #[serde(default)]
        formula_parts: Option<Vec<FormulaPartRecord>>,
        #[serde(default)]
        file_metadata: Vec<(String, String)>,
        #[serde(default)]
        import_source: Option<Box<ImportSourceRecord>>,
        #[serde(default)]
        x_prime: Option<f64>,
    },
    Graph {
        #[serde(default)]
        hidden: bool,
        label: String,
        comment: String,
        #[serde(default)]
        spec: PlotSpecRecord,
    },
    Matrix {
        #[serde(default)]
        hidden: bool,
        label: String,
        comment: String,
    },
    Note {
        #[serde(default)]
        hidden: bool,
        label: String,
        comment: String,
        text: String,
    },
}

impl From<KindRecordKnown> for KindRecord {
    fn from(value: KindRecordKnown) -> Self {
        match value {
            KindRecordKnown::Folder { children } => KindRecord::Folder { children },
            KindRecordKnown::Workbook {
                sheets,
                hidden,
                label,
                comment,
            } => KindRecord::Workbook {
                sheets,
                hidden,
                label,
                comment,
            },
            KindRecordKnown::Worksheet { columns } => KindRecord::Worksheet { columns },
            KindRecordKnown::Column {
                column_type,
                role,
                unit,
                long_name,
                comment,
                display_format,
                values,
                version,
                formula_parts,
                file_metadata,
                import_source,
                x_prime,
            } => KindRecord::Column {
                column_type,
                role,
                unit,
                long_name,
                comment,
                display_format,
                values,
                version,
                formula_parts,
                file_metadata,
                import_source,
                x_prime,
            },
            KindRecordKnown::Graph {
                hidden,
                label,
                comment,
                spec,
            } => KindRecord::Graph {
                hidden,
                label,
                comment,
                spec,
            },
            KindRecordKnown::Matrix {
                hidden,
                label,
                comment,
            } => KindRecord::Matrix {
                hidden,
                label,
                comment,
            },
            KindRecordKnown::Note {
                hidden,
                label,
                comment,
                text,
            } => KindRecord::Note {
                hidden,
                label,
                comment,
                text,
            },
        }
    }
}

impl From<RawVariant> for KindRecord {
    fn from(value: RawVariant) -> Self {
        KindRecord::Unknown(value)
    }
}

impl<'de> Deserialize<'de> for KindRecord {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserialize_with_fallback::<D, KindRecordKnown, KindRecord>(deserializer)
    }
}

/// A fragment of a column formula (`stokesforge_core::FormulaPart`):
/// typed text, or the raw identifier of the column read (#321).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) enum FormulaPartRecord {
    /// Operators, numbers, spaces, parentheses, as typed.
    Text(String),
    /// Identifier of a column read; bounded at read (`MAX_NODE_ID`).
    Column(u64),
}

/// Mirror of [`KindRecord`] **by reference**: serializes a known
/// variant without cloning its content — in particular the bytes of a
/// `Float` column, potentially large, which `Serialize` only needs to
/// read.
#[derive(Serialize)]
enum KindRecordRef<'a> {
    Folder {
        children: &'a [u64],
    },
    Workbook {
        sheets: &'a [u64],
        hidden: bool,
        label: &'a str,
        comment: &'a str,
    },
    Worksheet {
        columns: &'a [u64],
    },
    Column {
        column_type: TypeRecord,
        role: RoleRecord,
        unit: &'a str,
        long_name: &'a str,
        comment: &'a str,
        display_format: &'a str,
        values: &'a ValuesRecord,
        version: u64,
        formula_parts: &'a Option<Vec<FormulaPartRecord>>,
        file_metadata: &'a Vec<(String, String)>,
        import_source: &'a Option<Box<ImportSourceRecord>>,
        x_prime: Option<f64>,
    },
    Graph {
        hidden: bool,
        label: &'a str,
        comment: &'a str,
        spec: &'a PlotSpecRecord,
    },
    Matrix {
        hidden: bool,
        label: &'a str,
        comment: &'a str,
    },
    Note {
        hidden: bool,
        label: &'a str,
        comment: &'a str,
        text: &'a str,
    },
}

impl Serialize for KindRecord {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self {
            KindRecord::Folder { children } => {
                KindRecordRef::Folder { children }.serialize(serializer)
            }
            KindRecord::Workbook {
                sheets,
                hidden,
                label,
                comment,
            } => KindRecordRef::Workbook {
                sheets,
                hidden: *hidden,
                label,
                comment,
            }
            .serialize(serializer),
            KindRecord::Worksheet { columns } => {
                KindRecordRef::Worksheet { columns }.serialize(serializer)
            }
            KindRecord::Column {
                column_type,
                role,
                unit,
                long_name,
                comment,
                display_format,
                values,
                version,
                formula_parts,
                file_metadata,
                import_source,
                x_prime,
            } => KindRecordRef::Column {
                column_type: *column_type,
                role: *role,
                unit,
                long_name,
                comment,
                display_format,
                values,
                version: *version,
                formula_parts,
                file_metadata,
                import_source,
                x_prime: *x_prime,
            }
            .serialize(serializer),
            KindRecord::Graph {
                hidden,
                label,
                comment,
                spec,
            } => KindRecordRef::Graph {
                hidden: *hidden,
                label,
                comment,
                spec,
            }
            .serialize(serializer),
            KindRecord::Matrix {
                hidden,
                label,
                comment,
            } => KindRecordRef::Matrix {
                hidden: *hidden,
                label,
                comment,
            }
            .serialize(serializer),
            KindRecord::Note {
                hidden,
                label,
                comment,
                text,
            } => KindRecordRef::Note {
                hidden: *hidden,
                label,
                comment,
                text,
            }
            .serialize(serializer),
            KindRecord::Unknown(raw) => serialize_raw_variant(raw, serializer),
        }
    }
}

/// Data type of a column, flat.
///
/// `Mixed` only exists since the mixed-type columns. A file that
/// carries one stays readable by an **older** reader, but not
/// entirely: that reader does not know the variant name, its strict
/// decoding of `KindRecordKnown::Column` fails, and `unknown.rs`
/// files the node as [`KindRecord::Unknown`] — the loss is bounded to this
/// column, the rest of the project opens, and a rewrite restores it
/// identically (b20). This is exactly the degradation mechanism described at the
/// head of this module, applied here without any code of its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum TypeRecord {
    /// Numeric.
    Numeric,
    /// Date-time.
    DateTime,
    /// Mixed types: the type is carried by the cell.
    Mixed,
}

/// Plot role of a column, flat.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum RoleRecord {
    /// Abscissa.
    X,
    /// Ordinate.
    Y,
    /// Abscissa of the second layer (#167).
    X2,
    /// Ordinate of the second layer (#167).
    Y2,
    /// Abscissa of the third layer (#168).
    X3,
    /// Ordinate of the third layer (#168).
    Y3,
    /// Abscissa of the fourth layer (#168).
    X4,
    /// Ordinate of the fourth layer (#168).
    Y4,
    /// Third axis.
    Z,
    /// Error bar in X.
    XError,
    /// Error bar in Y.
    YError,
    /// Text label.
    Label,
    /// No plot role.
    None,
}

/// Values of a column, flat.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) enum ValuesRecord {
    /// Concatenated little-endian `f64` (8 bytes each, `NaN` preserved bit for
    /// bit): one raw byte per byte of data, never a MessagePack
    /// array of elements — this is what keeps the file size
    /// proportional to the data rather than to their encoding.
    Float(#[serde(with = "serde_bytes")] Vec<u8>),
    /// Mixed-type column, in **three parallel arrays** rather than an
    /// array of enums.
    ///
    /// The reason is the same as for `Float` above: `to_vec_named`
    /// serializes an enum by the **name** of its variant, so a
    /// `Vec<MixedCellRecord>` would write the string `"Number"` — seven bytes —
    /// in front of each of the eight bytes of a value. One tag byte per
    /// row, then only the values that exist, costs less than a
    /// text column today and keeps the `NaN`s bit for bit.
    Mixed {
        /// One tag per row, in order: `0` = empty cell,
        /// `1` = number, `2` = text. Any other byte makes the
        /// reading of the column fail (`ConvertError::BadMixedTag`): the three
        /// arrays cannot be read back if we do not know in which one to
        /// look for the next row.
        #[serde(with = "serde_bytes")]
        tags: Vec<u8>,
        /// The values of the rows tagged `1`, in order, eight bytes
        /// little-endian each — same encoding as `Float`.
        #[serde(with = "serde_bytes")]
        numbers: Vec<u8>,
        /// The values of the rows tagged `2`, in order.
        texts: Vec<String>,
    },
}

/// Tag of an empty cell in [`ValuesRecord::Mixed`].
pub(crate) const MIXED_TAG_EMPTY: u8 = 0;
/// Tag of a numeric cell in [`ValuesRecord::Mixed`].
pub(crate) const MIXED_TAG_NUMBER: u8 = 1;
/// Tag of a text cell in [`ValuesRecord::Mixed`].
pub(crate) const MIXED_TAG_TEXT: u8 = 2;

// -- PlotSpec (phase R3) ------------------------------------------------
//
// Same pattern as `Column`/`ColumnValues`/`ValuesRecord` above, extended to
// a tree of structures: one record per `stokesforge-plot` type,
// converted by hand by `convert.rs`, never derived from the model. The `NodeId`s
// referenced by a series travel as `u64`, like those of the child
// lists of `Folder`/`Workbook`/`Worksheet` further up — same
// representation, reused rather than reinvented.
//
// Each `Default` below reproduces *by hand* that of the matching
// `stokesforge-plot` type: this module still does not know
// `stokesforge-plot` (see the head doc of the file), so no shortcut of the
// kind `PlotSpecRecord::from(PlotSpec::default())`. A test in
// `convert.rs` locks that the two worlds stay in agreement.

/// [`stokesforge_plot::PlotSpec`], flat.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct PlotSpecRecord {
    pub(crate) title: String,
    /// Visibility of the title, independent of its content (#85).
    #[serde(default = "visible_by_default")]
    pub(crate) title_visible: bool,
    pub(crate) panels: Vec<PanelSpecRecord>,
    pub(crate) background: String,
    /// Added afterwards: absent from older files, brought back to the
    /// empty string (automatic color) by `#[serde(default)]`.
    #[serde(default)]
    pub(crate) title_color: String,
    /// Moved title (issue #53): absent from an older file, hence at its
    /// default place.
    #[serde(default)]
    pub(crate) title_anchor: Option<TextAnchorRecord>,
    /// Font and size of the title (issue #78): absent from an older
    /// file, hence those of the graph.
    #[serde(default)]
    pub(crate) title_font: TextFontRecord,
    /// Warning of out-of-date series (#198): absent from an older
    /// file, hence active.
    #[serde(default = "visible_by_default")]
    pub(crate) stale_warning: bool,
    /// Decimal separator of the ticks (#223): absent from an older
    /// file, hence the point.
    #[serde(default)]
    pub(crate) decimal_comma: bool,
}

/// [`stokesforge_plot::TextFont`], flat.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub(crate) struct TextFontRecord {
    #[serde(default)]
    pub(crate) family: String,
    #[serde(default)]
    pub(crate) size: Option<f64>,
    /// Width and alignment of the box (#244); absent from an older file.
    #[serde(default)]
    pub(crate) width: Option<f64>,
    #[serde(default)]
    pub(crate) align: TextAlignRecord,
}

/// [`stokesforge_plot::TextAnchor`], flat.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub(crate) struct TextAnchorRecord {
    pub(crate) x: f64,
    pub(crate) y: f64,
    pub(crate) axis: SeriesAxisRecord,
}

impl Default for PlotSpecRecord {
    fn default() -> Self {
        Self {
            title: String::new(),
            panels: vec![PanelSpecRecord::default()],
            background: "#ffffff00".to_string(), // stokesforge_plot::TRANSPARENT
            title_color: String::new(),
            title_visible: true,
            title_anchor: None,
            title_font: TextFontRecord::default(),
            stale_warning: true,
            decimal_comma: false,
        }
    }
}

/// [`stokesforge_plot::PanelSpec`], flat.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct PanelSpecRecord {
    pub(crate) title: String,
    /// Visibility of the title, independent of its content (#85).
    #[serde(default = "visible_by_default")]
    pub(crate) title_visible: bool,
    /// Moved title (issue #53), like `PlotSpecRecord::title_anchor`.
    #[serde(default)]
    pub(crate) title_anchor: Option<TextAnchorRecord>,
    /// Font and size of the panel title (issue #78).
    #[serde(default)]
    pub(crate) title_font: TextFontRecord,
    /// Color of the panel title (issue #90); absent from an older
    /// file, hence automatic.
    #[serde(default)]
    pub(crate) title_color: String,
    pub(crate) series: Vec<SeriesSpecRecord>,
    pub(crate) bottom: AxisSpecRecord,
    pub(crate) left: AxisSpecRecord,
    pub(crate) right: AxisSpecRecord,
    pub(crate) top: AxisSpecRecord,
    pub(crate) legend: LegendSpecRecord,
    /// Overlaid layer (#167); absent from an older file.
    #[serde(default)]
    pub(crate) overlay: Option<OverlaySpecRecord>,
    pub(crate) row: u64,
    pub(crate) column: u64,
    /// Free texts (issue #35): absent from an older file,
    /// `#[serde(default)]` brings them back to an empty list.
    #[serde(default)]
    pub(crate) annotations: Vec<AnnotationSpecRecord>,
    /// Graphic objects (issue #76): absent from an older file.
    #[serde(default)]
    pub(crate) shapes: Vec<ShapeSpecRecord>,
    /// Names of the series groups (#197); absent from an older file,
    /// hence all automatic.
    #[serde(default)]
    pub(crate) group_names: Vec<GroupNameRecord>,
    /// Color gradients kept by the series groups (#213); absent from an
    /// older file, hence no group has a color gradient.
    #[serde(default)]
    pub(crate) group_gradients: Vec<GroupGradientRecord>,
}

/// The stops of the color gradient of a series group (#213), flat.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct GroupGradientRecord {
    pub(crate) group: u32,
    pub(crate) stops: Vec<String>,
}

/// The name of a series group (#197), flat.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct GroupNameRecord {
    pub(crate) group: u32,
    pub(crate) name: String,
}

/// [`stokesforge_plot::OverlaySpec`], flat.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct OverlaySpecRecord {
    pub(crate) x_top: bool,
    pub(crate) y_right: bool,
    /// Number of the role layer (#168); absent from older files:
    /// 0, layer without a role.
    #[serde(default)]
    pub(crate) layer: u8,
}

/// [`stokesforge_plot::ShapeKind`], flat.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum ShapeKindRecord {
    Rectangle,
    Ellipse,
    Line,
    Arrow,
}

/// [`stokesforge_plot::ShapeSpec`], flat.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct ShapeSpecRecord {
    #[serde(default = "visible_by_default")]
    pub(crate) visible: bool,
    pub(crate) kind: ShapeKindRecord,
    pub(crate) x1: f64,
    pub(crate) y1: f64,
    pub(crate) x2: f64,
    pub(crate) y2: f64,
    pub(crate) axis: SeriesAxisRecord,
    #[serde(default)]
    pub(crate) line_color: String,
    pub(crate) line_width: f64,
    #[serde(default)]
    pub(crate) fill_color: String,
}

/// [`stokesforge_plot::AnnotationSpec`], flat.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct AnnotationSpecRecord {
    #[serde(default = "visible_by_default")]
    pub(crate) visible: bool,
    pub(crate) text: String,
    pub(crate) x: f64,
    pub(crate) y: f64,
    pub(crate) axis: SeriesAxisRecord,
    pub(crate) color: String,
    pub(crate) size: f64,
    pub(crate) rotation: f64,
    /// Font of the text (issue #78); absent from an older file.
    #[serde(default)]
    pub(crate) font: String,
    /// Width as a fraction of the figure (#244); absent = one line.
    #[serde(default)]
    pub(crate) width: Option<f64>,
    /// Alignment in the box (#244); absent = centered.
    #[serde(default)]
    pub(crate) align: TextAlignRecord,
}

/// [`stokesforge_plot::TextAlign`], flat.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum TextAlignRecord {
    Left,
    #[default]
    Center,
    Right,
}

impl Default for PanelSpecRecord {
    fn default() -> Self {
        Self {
            title: String::new(),
            title_visible: true,
            title_anchor: None,
            title_font: TextFontRecord::default(),
            title_color: String::new(),
            series: Vec::new(),
            bottom: AxisSpecRecord::default(),
            left: AxisSpecRecord::default(),
            right: AxisSpecRecord::default(),
            top: AxisSpecRecord::hidden(),
            legend: LegendSpecRecord::default(),
            overlay: None,
            row: 0,
            column: 0,
            annotations: Vec::new(),
            shapes: Vec::new(),
            group_names: Vec::new(),
            group_gradients: Vec::new(),
        }
    }
}

/// [`stokesforge_plot::AxisSpec`], flat.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct AxisSpecRecord {
    pub(crate) label: String,
    /// Visibility of the title, independent of its content (#85).
    #[serde(default = "visible_by_default")]
    pub(crate) title_visible: bool,
    pub(crate) unit: String,
    pub(crate) scale: AxisScaleRecord,
    pub(crate) auto_range: bool,
    pub(crate) range_min: Option<f64>,
    pub(crate) range_max: Option<f64>,
    pub(crate) grid: bool,
    pub(crate) visible: bool,
    pub(crate) inverted: bool,
    /// Added afterwards, like `PlotSpecRecord::title_color`.
    #[serde(default)]
    pub(crate) title_color: String,
    /// Moved title (issue #53), like `PlotSpecRecord::title_anchor`.
    #[serde(default)]
    pub(crate) title_anchor: Option<TextAnchorRecord>,
    /// Font and size of the axis title (issue #78).
    #[serde(default)]
    pub(crate) title_font: TextFontRecord,
    /// Added afterwards, like `PlotSpecRecord::title_color`.
    #[serde(default)]
    pub(crate) tick_color: String,
    /// Mirror axis (#176).
    #[serde(default)]
    pub(crate) mirror: bool,
    /// Tick labels (#176).
    #[serde(default = "visible_by_default")]
    pub(crate) tick_labels: bool,
    /// Ticks (#344); true by default, otherwise the old
    /// files would lose their ticks.
    #[serde(default = "visible_by_default")]
    pub(crate) ticks: bool,
    /// Direction of the ticks (#344).
    #[serde(default)]
    pub(crate) tick_direction: TickDirectionRecord,
    /// Minor ticks (#344); false by default: an old file had none.
    #[serde(default)]
    pub(crate) minor_ticks: bool,
    /// Minor ticks between two major ones (#344); absent = automatic.
    #[serde(default)]
    pub(crate) minor_tick_count: Option<u8>,
    /// One number out of n written (#344); 1 by default.
    #[serde(default = "one")]
    pub(crate) label_every: u8,
    /// Length of the major ticks, in pixels (#344, "Advanced"); absent =
    /// automatic. The raw value is kept: the renderer bounds it.
    #[serde(default)]
    pub(crate) tick_length: Option<f64>,
    /// Length of the minor ticks; absent = half the major ones.
    #[serde(default)]
    pub(crate) minor_tick_length: Option<f64>,
    /// Thickness of the ticks, in points; absent = automatic.
    #[serde(default)]
    pub(crate) tick_width: Option<f64>,
    /// Color of the ticks; empty = that of the scale.
    #[serde(default)]
    pub(crate) tick_mark_color: String,
}

impl AxisSpecRecord {
    /// Hidden axis — the default value of the top axis of a panel
    /// (`AxisSpec::hidden` on the `stokesforge-plot` side).
    fn hidden() -> Self {
        Self {
            visible: false,
            ..Self::default()
        }
    }
}

impl Default for AxisSpecRecord {
    fn default() -> Self {
        Self {
            label: String::new(),
            unit: String::new(),
            scale: AxisScaleRecord::Linear,
            auto_range: true,
            range_min: None,
            range_max: None,
            grid: false,
            visible: true,
            inverted: false,
            title_color: String::new(),
            title_visible: true,
            title_anchor: None,
            title_font: TextFontRecord::default(),
            tick_color: String::new(),
            mirror: false,
            tick_labels: true,
            ticks: true,
            tick_direction: TickDirectionRecord::Out,
            // An axis absent from the file takes the defaults of a new axis; an axis
            // present without the field (`#[serde(default)]`) had none.
            minor_ticks: true,
            minor_tick_count: None,
            label_every: 1,
            tick_length: None,
            minor_tick_length: None,
            tick_width: None,
            tick_mark_color: String::new(),
        }
    }
}

/// [`stokesforge_plot::TickDirection`], flat. An unknown value reads as
/// `Out` (graceful degradation, invariant 10).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub(crate) enum TickDirectionRecord {
    In,
    Both,
    #[default]
    #[serde(other)]
    Out,
}

/// [`stokesforge_plot::AxisScale`], flat.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum AxisScaleRecord {
    Linear,
    Log,
}

/// [`stokesforge_plot::SeriesSpec`], flat. The referenced columns travel as
/// raw `u64`, as everywhere else in this schema — a `NodeId` whether the
/// targeted column survived the file or not: a dangling reference
/// is not a load error, it is discovered at resolution
/// (`stokesforge_core::resolve`), never before.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct SeriesSpecRecord {
    pub(crate) y: u64,
    pub(crate) x: Option<u64>,
    pub(crate) x_error: Option<u64>,
    pub(crate) y_error: Option<u64>,
    pub(crate) plot_type: PlotTypeRecord,
    pub(crate) name: String,
    pub(crate) line: LineSpecRecord,
    pub(crate) symbol: SymbolSpecRecord,
    pub(crate) axis: SeriesAxisRecord,
    pub(crate) visible: bool,
    pub(crate) show_in_legend: bool,
    pub(crate) bar_width: Option<f64>,
    pub(crate) histogram_bins: u32,
    /// `SeriesSpec::row` (#166). Absent from earlier files.
    #[serde(default)]
    pub(crate) row: Option<u64>,
    /// `SeriesSpec::group` (#177). Absent from earlier files.
    #[serde(default)]
    pub(crate) group: Option<u32>,
}

/// [`stokesforge_plot::PlotType`], flat.
///
/// `Serialize`/`Deserialize` written by hand, like [`KindRecord`] (C5,
/// level 2): a plot type that a future version would add becomes
/// [`PlotTypeRecord::Unknown`] rather than making the whole graph that
/// carries it fail — and the whole file with it, before this fix.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum PlotTypeRecord {
    Scatter,
    Line,
    LineScatter,
    Bar,
    Histogram,
    ErrorBar,
    /// Plot type of a newer version of StokesForge.
    Unknown(RawVariant),
}

/// Strict mirror of [`PlotTypeRecord`] (without `Unknown`) — same role as
/// `KindRecordKnown`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
enum PlotTypeRecordKnown {
    Scatter,
    Line,
    LineScatter,
    Bar,
    Histogram,
    ErrorBar,
}

impl From<PlotTypeRecordKnown> for PlotTypeRecord {
    fn from(value: PlotTypeRecordKnown) -> Self {
        match value {
            PlotTypeRecordKnown::Scatter => PlotTypeRecord::Scatter,
            PlotTypeRecordKnown::Line => PlotTypeRecord::Line,
            PlotTypeRecordKnown::LineScatter => PlotTypeRecord::LineScatter,
            PlotTypeRecordKnown::Bar => PlotTypeRecord::Bar,
            PlotTypeRecordKnown::Histogram => PlotTypeRecord::Histogram,
            PlotTypeRecordKnown::ErrorBar => PlotTypeRecord::ErrorBar,
        }
    }
}

impl From<RawVariant> for PlotTypeRecord {
    fn from(value: RawVariant) -> Self {
        PlotTypeRecord::Unknown(value)
    }
}

impl<'de> Deserialize<'de> for PlotTypeRecord {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserialize_with_fallback::<D, PlotTypeRecordKnown, PlotTypeRecord>(deserializer)
    }
}

impl Serialize for PlotTypeRecord {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let known = match self {
            PlotTypeRecord::Scatter => PlotTypeRecordKnown::Scatter,
            PlotTypeRecord::Line => PlotTypeRecordKnown::Line,
            PlotTypeRecord::LineScatter => PlotTypeRecordKnown::LineScatter,
            PlotTypeRecord::Bar => PlotTypeRecordKnown::Bar,
            PlotTypeRecord::Histogram => PlotTypeRecordKnown::Histogram,
            PlotTypeRecord::ErrorBar => PlotTypeRecordKnown::ErrorBar,
            PlotTypeRecord::Unknown(raw) => return serialize_raw_variant(raw, serializer),
        };
        known.serialize(serializer)
    }
}

/// [`stokesforge_plot::SeriesAxis`], flat.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum SeriesAxisRecord {
    Left,
    Right,
}

/// [`stokesforge_plot::LineSpec`], flat.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct LineSpecRecord {
    pub(crate) color: String,
    pub(crate) style: LineStyleRecord,
    pub(crate) width: f64,
}

/// [`stokesforge_plot::LineStyle`], flat.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum LineStyleRecord {
    None,
    Solid,
    Dash,
    Dot,
    DashDot,
}

/// [`stokesforge_plot::SymbolSpec`], flat.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct SymbolSpecRecord {
    pub(crate) shape: SymbolShapeRecord,
    pub(crate) size: f64,
    pub(crate) color: String,
    pub(crate) edge_color: String,
}

/// [`stokesforge_plot::SymbolShape`], flat.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum SymbolShapeRecord {
    None,
    Circle,
    Square,
    Triangle,
    Diamond,
    Plus,
    Cross,
}

/// [`stokesforge_plot::LegendSpec`], flat.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct LegendSpecRecord {
    pub(crate) visible: bool,
    pub(crate) position: LegendPositionRecord,
    /// Legend moved (issue #53), like `PlotSpecRecord::title_anchor`.
    #[serde(default)]
    pub(crate) anchor: Option<TextAnchorRecord>,
    /// Font and size of the legend (issue #77).
    #[serde(default)]
    pub(crate) font: TextFontRecord,
    /// Color of the series names (issue #90); absent from an older
    /// file, hence automatic.
    #[serde(default)]
    pub(crate) color: String,
}

impl Default for LegendSpecRecord {
    fn default() -> Self {
        // `LegendSpec::default()`: visible, anchored at the top right — no
        // `#[derive(Default)]` here precisely because `bool::default()`
        // (`false`) would contradict this default value.
        Self {
            visible: true,
            position: LegendPositionRecord::default(),
            anchor: None,
            font: TextFontRecord::default(),
            color: String::new(),
        }
    }
}

/// [`stokesforge_plot::LegendPosition`], flat.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub(crate) enum LegendPositionRecord {
    TopLeft,
    #[default]
    TopRight,
    BottomLeft,
    BottomRight,
}

// The objects of earlier files were all visible.
fn visible_by_default() -> bool {
    true
}

// An axis of an earlier file wrote all its numbers (#344).
fn one() -> u8 {
    1
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An unknown tick direction (file of a newer version) reads as
    /// "outward", without failure (invariant 10).
    #[test]
    fn an_unknown_tick_direction_reads_as_outward() {
        #[derive(Serialize)]
        enum Future {
            Diagonal,
            In,
        }
        let unknown = rmp_serde::to_vec_named(&Future::Diagonal).expect("encoding");
        let reloaded: TickDirectionRecord = rmp_serde::from_slice(&unknown).expect("decoding");
        assert_eq!(reloaded, TickDirectionRecord::Out);
        let known = rmp_serde::to_vec_named(&Future::In).expect("encoding");
        let reloaded: TickDirectionRecord = rmp_serde::from_slice(&known).expect("decoding");
        assert_eq!(reloaded, TickDirectionRecord::In);
    }

    /// A file written before the addition of the text colors (graph
    /// title, title and scale of the axes) reads back: the absent fields
    /// come back empty, that is to say in automatic color.
    #[test]
    fn a_spec_from_before_the_text_colors_reads_back_as_automatic() {
        #[derive(Serialize)]
        struct OldAxis {
            label: String,
            unit: String,
            scale: AxisScaleRecord,
            auto_range: bool,
            range_min: Option<f64>,
            range_max: Option<f64>,
            grid: bool,
            visible: bool,
            inverted: bool,
        }
        #[derive(Serialize)]
        struct OldSpec {
            title: String,
            panels: Vec<PanelSpecRecord>,
            background: String,
        }
        let axis = OldAxis {
            label: "Time".to_string(),
            unit: "s".to_string(),
            scale: AxisScaleRecord::Linear,
            auto_range: true,
            range_min: None,
            range_max: None,
            grid: false,
            visible: true,
            inverted: false,
        };
        let bytes = rmp_serde::to_vec_named(&axis).expect("encoding");
        let reloaded: AxisSpecRecord = rmp_serde::from_slice(&bytes).expect("decoding");
        assert_eq!(reloaded.label, "Time");
        assert_eq!(reloaded.title_color, "");
        assert_eq!(reloaded.tick_color, "");
        assert!(
            reloaded.title_visible,
            "an old axis keeps its title visible"
        );
        // #344: an old axis keeps its ticks, outward.
        assert!(reloaded.ticks, "an old axis keeps its ticks");
        assert_eq!(reloaded.tick_direction, TickDirectionRecord::Out);
        assert!(!reloaded.minor_ticks, "an old axis has no minor ticks");
        assert_eq!(reloaded.minor_tick_count, None);
        assert_eq!(
            reloaded.label_every, 1,
            "an old axis writes all its numbers"
        );
        // #344: the "Advanced" tick settings are absent = automatic.
        assert_eq!(reloaded.tick_length, None);
        assert_eq!(reloaded.minor_tick_length, None);
        assert_eq!(reloaded.tick_width, None);
        assert_eq!(reloaded.tick_mark_color, "");

        let spec = OldSpec {
            title: "Old".to_string(),
            panels: vec![PanelSpecRecord::default()],
            background: "#000000".to_string(),
        };
        let bytes = rmp_serde::to_vec_named(&spec).expect("encoding");
        let reloaded: PlotSpecRecord = rmp_serde::from_slice(&bytes).expect("decoding");
        assert_eq!(reloaded.title, "Old");
        assert!(
            reloaded.title_visible,
            "an old graph keeps its title visible"
        );
        assert_eq!(reloaded.title_color, "");
        assert!(
            reloaded.stale_warning,
            "an old graph keeps the out-of-date data warning"
        );
    }
}

#[cfg(test)]
mod visibility_tests {
    use super::*;
    #[test]
    fn earlier_objects_without_a_visible_field_read_back_visible() {
        #[derive(Serialize)]
        struct OldText {
            text: String,
            x: f64,
            y: f64,
            axis: SeriesAxisRecord,
            color: String,
            size: f64,
            rotation: f64,
        }
        #[derive(Serialize)]
        struct OldShape {
            kind: ShapeKindRecord,
            x1: f64,
            y1: f64,
            x2: f64,
            y2: f64,
            axis: SeriesAxisRecord,
            line_width: f64,
        }
        let bytes = rmp_serde::to_vec_named(&OldText {
            text: "Peak".into(),
            x: 1.0,
            y: 2.0,
            axis: SeriesAxisRecord::Left,
            color: String::new(),
            size: 16.0,
            rotation: 0.0,
        })
        .unwrap();
        let text: AnnotationSpecRecord = rmp_serde::from_slice(&bytes).unwrap();
        let bytes = rmp_serde::to_vec_named(&OldShape {
            kind: ShapeKindRecord::Rectangle,
            x1: 0.0,
            y1: 0.0,
            x2: 1.0,
            y2: 1.0,
            axis: SeriesAxisRecord::Left,
            line_width: 1.5,
        })
        .unwrap();
        let shape: ShapeSpecRecord = rmp_serde::from_slice(&bytes).unwrap();
        assert!(text.visible);
        assert!(shape.visible);
    }
}

#[cfg(test)]
mod title_visibility_tests {
    use super::*;
    #[test]
    fn hidden_titles_and_old_absent_fields() {
        let mut record = PlotSpecRecord {
            title_visible: false,
            ..Default::default()
        };
        record.panels[0].title_visible = false;
        record.panels[0].bottom.title_visible = false;
        let bytes = rmp_serde::to_vec_named(&record).unwrap();
        let reloaded: PlotSpecRecord = rmp_serde::from_slice(&bytes).unwrap();
        assert_eq!(reloaded, record);
        let mut value = rmpv::decode::read_value(&mut std::io::Cursor::new(bytes)).unwrap();
        fn remove_field(value: &mut rmpv::Value) {
            match value {
                rmpv::Value::Map(fields) => {
                    fields.retain(|(key, _)| key.as_str() != Some("title_visible"));
                    for (_, value) in fields {
                        remove_field(value);
                    }
                }
                rmpv::Value::Array(values) => {
                    for value in values {
                        remove_field(value);
                    }
                }
                _ => {}
            }
        }
        remove_field(&mut value);
        let mut old = Vec::new();
        rmpv::encode::write_value(&mut old, &value).unwrap();
        let reloaded: PlotSpecRecord = rmp_serde::from_slice(&old).unwrap();
        assert!(reloaded.title_visible);
        assert!(reloaded.panels[0].title_visible);
        for side in [
            &reloaded.panels[0].bottom,
            &reloaded.panels[0].left,
            &reloaded.panels[0].right,
            &reloaded.panels[0].top,
        ] {
            assert!(side.title_visible);
        }
    }
}

/// Geometry of a window, as it is written in the file
/// (`crate::WindowGeometry` on the API side).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub(crate) struct WindowGeometryRecord {
    pub(crate) x: f64,
    pub(crate) y: f64,
    pub(crate) width: f64,
    pub(crate) height: f64,
    /// `None`: never saved, the application applies its default
    /// layout.
    #[serde(default)]
    pub(crate) view: Option<WindowViewRecord>,
}

/// Display state of a window (`crate::WindowView`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct WindowViewRecord {
    pub(crate) mode: WindowModeRecord,
    #[serde(default)]
    pub(crate) collapsed: bool,
    #[serde(default)]
    pub(crate) stack: u32,
}

/// Mode of a window (`crate::WindowMode`). An unknown value reads as
/// `Closed`: graceful degradation, never a load failure
/// (invariant 10).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum WindowModeRecord {
    Open,
    Maximized,
    #[serde(other)]
    Closed,
}

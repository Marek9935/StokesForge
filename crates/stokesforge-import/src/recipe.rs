//! The recipe model: the frozen (serializable) description of how a given
//! device writes its files, without knowing anything about the parsing engine
//! itself (see [`crate::parse`]).

/// A reusable parsing recipe for a device file format.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ImportRecipe {
    pub name: String,
    pub separator: Separator,
    pub decimal: DecimalStyle,
    pub header: HeaderSkip,
    pub footer_skip_lines: usize,
    pub has_column_header_row: bool,
    pub name_source: NameSource,
    /// Split of the header into "main" header and "sub-header", in the manner
    /// of OriginLab's ASCII import wizard (the "import preview" work package);
    /// see [`HeaderLayout`]. `Default` (zero everywhere) leaves `header`
    /// (above) fully in charge, as before that work package: the two models
    /// overlap only if the caller sets `header_layout` to anything other than
    /// zero. `#[serde(default)]` so that a JSON recipe saved before that work
    /// package reloads identically (`RecipeStore`, see its top-level doc).
    #[serde(default)]
    pub header_layout: HeaderLayout,
    /// Assignment of the `long_name`/`unit`/`comment` roles to ranges of
    /// sub-header lines; see [`SubheaderRoles`]. Has no effect as long as
    /// `header_layout` defines no sub-header line.
    #[serde(default)]
    pub subheader_roles: SubheaderRoles,
    /// Number of characters to remove at the start of each data line before
    /// parsing (fixed prefixes of some instrument formats).
    #[serde(default)]
    pub skip_chars: usize,
    /// Number of data lines to annotate in the live preview
    /// ([`crate::preview::preview`]). Never affects [`crate::parse`], which
    /// always parses the whole file. Caps the preview's work regardless of the
    /// real file size.
    #[serde(default = "default_preview_lines")]
    pub preview_lines: usize,
    /// Where the data stops (the rest is the footer); see [`DataEnd`]. Applies
    /// **after** `footer_skip_lines`, which still removes a fixed number of
    /// lines at the end of the file.
    #[serde(default)]
    pub data_end: DataEnd,
    /// Keep the file header as metadata of each column (true by default:
    /// everything is kept). See [`crate::metadata`].
    #[serde(default = "default_true")]
    pub keep_header_metadata: bool,
    /// Also keep the footer (the lines after the end of the data,
    /// [`DataEnd`]) as metadata. False by default.
    #[serde(default)]
    pub keep_footer_metadata: bool,
    /// Keys or sections **not** to keep (a key name, or a section title in
    /// brackets: the whole section is then dropped), compared ignoring case
    /// and surrounding whitespace.
    #[serde(default)]
    pub excluded_metadata: Vec<String>,
    /// Role chosen for each column of the file, by position ("Columns" step of
    /// the wizard). Shorter than the number of columns: the following columns
    /// are [`ColumnRoleChoice::Auto`].
    #[serde(default)]
    pub column_roles: Vec<ColumnRoleChoice>,
    /// Thousands separator to ignore in numbers (`' '`, `','`, `'.'`,
    /// `'\''`...), `None` if there is none.
    #[serde(default)]
    pub thousands: Option<char>,
    /// Field order of a date with slashes or dots (`03/04/2026`): day first
    /// (default) or month first. No effect on `YYYY-MM-DD`.
    #[serde(default)]
    pub date_order: DateOrder,
    /// Tokens that mean "missing value" (`--`, `n/a`, `*`, `NaN`...): the cell
    /// stays empty instead of becoming text (and a mixed column).
    #[serde(default)]
    pub missing_tokens: Vec<String>,
    /// Prefix of comment lines to ignore in the data zone (`#`, `;`, `//`...),
    /// empty if there is none. A line that starts (after its spaces) with this
    /// prefix is never data.
    #[serde(default)]
    pub comment_prefix: String,
    /// Number of data lines to ignore at the start (after the header and the
    /// column names line): together with [`DataEnd`], the range of lines to
    /// import.
    #[serde(default)]
    pub skip_data_rows: usize,
    /// Encoding of the file; `Auto` detects it ([`crate::decode`]).
    #[serde(default)]
    pub encoding: Encoding,
    /// Criteria for automatically choosing this template from a file; see
    /// [`crate::choose_recipe`]. Empty: never chosen automatically.
    #[serde(default)]
    pub auto_match: AutoMatch,
    /// Forced kind of each column, by position ("Columns" step). Shorter than
    /// the number of columns: the following ones are `Auto`.
    #[serde(default)]
    pub column_kinds: Vec<ColumnKind>,
    /// Where the imported columns land in the active workbook; see
    /// [`ImportMode`]. No effect for a new workbook.
    #[serde(default)]
    pub import_mode: ImportMode,
    /// How several files are distributed into sheets; see [`FilesLayout`].
    #[serde(default)]
    pub files_layout: FilesLayout,
    /// How a file that contains **several data blocks** (one spectrum after
    /// the other) is split; see [`BlockSplit`]. Each block is parsed with the
    /// same recipe, its own header included.
    #[serde(default)]
    pub block_split: BlockSplit,
}

/// Split of a file into independent data blocks.
#[derive(Debug, Clone, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum BlockSplit {
    /// A single block: the whole file.
    #[default]
    None,
    /// A new block starts at each line that begins with this text (ignoring
    /// case, surrounding whitespace removed), for example `TITLE` for a Jasco
    /// file that chains several spectra, each with its header.
    BeforeLine(String),
    /// Blocks are separated by one or more empty lines.
    BlankLines,
}

/// Import mode into the active workbook, like Origin's options.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum ImportMode {
    /// A new sheet (the original behavior).
    #[default]
    NewSheet,
    /// Replaces the content of the active sheet.
    ReplaceSheet,
    /// Appends columns after those of the active sheet.
    AddColumns,
    /// Appends rows after those of the active sheet: column `j` of the file
    /// extends column `j` of the sheet.
    AddRows,
}

/// Distribution of several files into sheets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum FilesLayout {
    /// One sheet per file.
    #[default]
    SheetPerFile,
    /// All files in the same sheet, side by side (X Y X Y...).
    SideBySide,
    /// All files in the same sheet with the **X of the first file shared**:
    /// one X column, then the other columns of each file.
    SharedX,
}

/// Kind of an imported column.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum ColumnKind {
    /// Numbers; an unreadable token is kept as text (mixed column).
    #[default]
    Auto,
    /// Text: each non-empty token is kept as is, even if it looks like a
    /// number (codes, sample identifiers...).
    Text,
    /// Dates and times, read as POSIX timestamps ([`crate::datetime`]); a
    /// token that is not a date stays text.
    DateTime,
}

/// Criteria that a file must **all** meet (the non-empty ones) for a template
/// to be proposed automatically: extension, part of the name, signature in the
/// start of the content (like `ORIGIN;JASCO`). The principle of Origin's
/// "Import Filters".
#[derive(Debug, Clone, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub struct AutoMatch {
    /// Accepted extensions, without dot, ignoring case (`csv`, `txt`...).
    #[serde(default)]
    pub extensions: Vec<String>,
    /// Text the file name must contain (ignoring case).
    #[serde(default)]
    pub name_contains: String,
    /// Text the start of the file must contain (ignoring case).
    #[serde(default)]
    pub content_contains: String,
}

impl AutoMatch {
    /// No criterion: the template is never chosen automatically.
    pub fn is_empty(&self) -> bool {
        self.extensions.is_empty()
            && self.name_contains.trim().is_empty()
            && self.content_contains.trim().is_empty()
    }
}

impl ImportRecipe {
    /// The recipe as readable JSON: the file you hand to a colleague.
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_default()
    }

    /// Reads back a recipe exported by [`ImportRecipe::to_json`]. `Err` with a
    /// readable message if the text is not a recipe.
    pub fn from_json(text: &str) -> Result<Self, String> {
        serde_json::from_str(text).map_err(|err| format!("unreadable template: {err}"))
    }
}

/// Day/month order of a date `DD/MM/YYYY` or `MM/DD/YYYY` (recipe setting).
/// Day first is the project rule (#217) and the default.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum DateOrder {
    /// `03/04/2026` = April 3, 2026.
    #[default]
    DayFirst,
    /// `03/04/2026` = March 4, 2026.
    MonthFirst,
}

/// Encoding forced on a file, when automatic detection gets it wrong.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum Encoding {
    /// UTF-16 with a mark, otherwise UTF-8 (an isolated invalid byte is read
    /// as Windows-1252: mixed decoding, see [`crate::decode`]).
    #[default]
    Auto,
    /// UTF-8 (a byte order mark is removed; an invalid byte becomes `U+FFFD`
    /// instead of failing the import).
    Utf8,
    /// Windows-1252, the code page of devices on Windows.
    Windows1252,
    /// ISO 8859-1.
    Latin1,
    /// UTF-16 little-endian, with or without a mark.
    Utf16Le,
    /// UTF-16 big-endian, with or without a mark.
    Utf16Be,
}

/// Role of an imported column, as chosen in the wizard.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum ColumnRoleChoice {
    /// The first column is the abscissa (X), the others are Y.
    #[default]
    Auto,
    /// Abscissa.
    X,
    /// Ordinate.
    Y,
    /// X error bar.
    XError,
    /// Y error bar.
    YError,
    /// Column not imported.
    Ignore,
}

fn default_true() -> bool {
    true
}

/// End of a file's data zone. The lines after the cut form the footer: never
/// imported, but shown by the preview ([`crate::preview::LineRole::Footer`])
/// and kept by [`crate::parse::ParsedTable::footer_lines`].
#[derive(Debug, Clone, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum DataEnd {
    /// Automatic cut. It arms at the first line whose first column is a
    /// number; once armed, it cuts at the first line whose first column (the
    /// X) is not a number. An empty line cuts too. A text cell in another
    /// column does not cut.
    ///
    /// **Not the default**: a missing measurement ("n/a", "-", "NaN") in X
    /// stops the reading and silently loses all the following lines. To
    /// choose for a device known to write a footer (the Jasco templates);
    /// recipes already saved with `"data_end":"Auto"` keep cutting.
    Auto,
    /// No cut: the data runs to the end of the file. Default.
    #[default]
    None,
    /// Cuts at the first empty (or blank) line.
    FirstBlankLine,
    /// Cuts at the first line equal to this marker (case-insensitive
    /// comparison, surrounding whitespace removed); that line is excluded.
    Marker(String),
    /// Keeps only the first `n` data lines.
    LinesAfterHeader(usize),
    /// Removes the last `n` lines (like `footer_skip_lines`).
    LinesBeforeEnd(usize),
}

fn default_preview_lines() -> usize {
    20
}

/// Range of sub-header lines, 1-based and inclusive at both ends, **relative
/// to the sub-header block** (so independent of the number of main header
/// lines before it): the "from ... to ..." of OriginLab's wizard for a given
/// role.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SubheaderLineRange {
    pub from: usize,
    pub to: usize,
}

impl SubheaderLineRange {
    /// A single-line range, by far the most common case (the reference
    /// OriginLab example is `18` to `18`).
    pub fn single(line: usize) -> Self {
        Self {
            from: line,
            to: line,
        }
    }
}

/// How many "main" header lines and "sub-header" lines the file has, in the
/// manner of OriginLab's ASCII import wizard. Distinct from
/// `ImportRecipe::header` (which only knows a single block to skip): only the
/// sub-header can carry roles assigned per line ([`SubheaderRoles`]). That is
/// the only reason for the distinction; StokesForge does not treat the content
/// of the main header other than to skip it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub struct HeaderLayout {
    pub main_header_lines: usize,
    pub subheader_lines: usize,
    /// When checked, the tool recomputes these two counts from the file
    /// content instead of keeping them as entered; see
    /// `crate::preview::detect_header_layout`. This field does nothing by
    /// itself: it is the calling layer (`stokesforge-app`) that, seeing it
    /// checked, calls the detection and rewrites `main_header_lines`/
    /// `subheader_lines`. Like OriginLab's re-evaluation button, the detection
    /// is not applied continuously at every keystroke.
    pub auto_detect: bool,
}

/// Column separator.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Separator {
    Comma,
    Semicolon,
    Tab,
    Whitespace,
    /// Any character of choice (`|`, `:`...).
    Custom(char),
    /// Fixed-width columns: the width, in characters, of each one except the
    /// last, which takes the rest of the line.
    FixedWidth(Vec<usize>),
    /// Detected automatically from the first data line.
    Auto,
}

/// Decimal separator convention.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum DecimalStyle {
    Dot,
    Comma,
    /// Detected from the first data lines: comma if it appears there between
    /// two digits together with another column separator (`;`, tab), dot
    /// otherwise.
    Auto,
}

/// How many lines to skip before the data.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum HeaderSkip {
    None,
    /// Skips the first `n` lines, whatever their content.
    Lines(usize),
    /// Skips everything up to the first line whose text, with surrounding
    /// whitespace removed, equals this marker (case-insensitive comparison);
    /// that line itself is skipped too.
    UntilMarker(String),
}

/// Where the name of the imported series comes from.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum NameSource {
    /// The file name (without extension), supplied by the caller.
    Filename,
    /// A value read from the skipped header lines: looks for a
    /// `KEY<separator>VALUE` line where KEY matches this string
    /// (case-insensitive, surrounding whitespace ignored on both sides), and
    /// takes VALUE (trimmed) as the name.
    HeaderKey(String),
}

/// Assignment of the `long_name`/`unit`/`comment`/`x_prime` roles of
/// [`stokesforge_core::ColumnMeta`] (the crate does not depend on
/// `stokesforge-core`, see the top-level doc of [`crate`]; the role names stay
/// those of `ColumnMeta`, not OriginLab's, precisely so the mapping is direct
/// on the `stokesforge-app` side) to ranges of sub-header lines.
///
/// Has no equivalent for OriginLab's "System Parameters"/"User Parameters",
/// deliberately: see `docs/chantier-import-previsualisation.md` (a decision
/// not to reopen without a new reason).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub struct SubheaderRoles {
    pub long_name: Option<SubheaderLineRange>,
    pub unit: Option<SubheaderLineRange>,
    pub comment: Option<SubheaderLineRange>,
    /// Line whose every numeric token becomes the x' value
    /// ([`stokesforge_core::ColumnMeta::x_prime`], #164) of its column: the
    /// time of a spectrum, in a kinetic series.
    #[serde(default)]
    pub x_prime: Option<SubheaderLineRange>,
    /// When checked, `unit` is ignored: each token of the `long_name` line is
    /// read as "Name (unit)", the unit being what follows the first opening
    /// parenthesis up to the first closing one. A token without recognized
    /// parentheses keeps all its text as the long name and gives an empty
    /// unit, rather than cutting arbitrarily (see `crate::parse::parse`).
    pub same_line_name_unit: bool,
}

impl ImportRecipe {
    /// Kind chosen for column `index` (`Auto` by default).
    pub fn column_kind(&self, index: usize) -> ColumnKind {
        self.column_kinds.get(index).copied().unwrap_or_default()
    }

    /// Effective role of column `index`: the explicit choice, otherwise X for
    /// the first column and Y for the following ones.
    pub fn column_role(&self, index: usize) -> ColumnRoleChoice {
        match self.column_roles.get(index).copied().unwrap_or_default() {
            ColumnRoleChoice::Auto if index == 0 => ColumnRoleChoice::X,
            ColumnRoleChoice::Auto => ColumnRoleChoice::Y,
            explicit => explicit,
        }
    }

    /// Brings the two competing header settings (`header` and
    /// `header_layout`) down to one: `HeaderSkip::Lines(n)` becomes `n` main
    /// header lines. `UntilMarker` stays as is. No effect if the structured
    /// split is already used.
    pub fn unify_header(&mut self) {
        if self.header_layout.main_header_lines == 0 && self.header_layout.subheader_lines == 0 {
            if let HeaderSkip::Lines(n) = self.header {
                self.header_layout.main_header_lines = n;
                self.header = HeaderSkip::None;
            }
        }
    }

    /// New recipe with the most permissive settings.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            separator: Separator::Auto,
            decimal: DecimalStyle::Dot,
            header: HeaderSkip::None,
            footer_skip_lines: 0,
            has_column_header_row: false,
            name_source: NameSource::Filename,
            header_layout: HeaderLayout::default(),
            subheader_roles: SubheaderRoles::default(),
            skip_chars: 0,
            preview_lines: default_preview_lines(),
            data_end: DataEnd::default(),
            keep_header_metadata: true,
            keep_footer_metadata: false,
            excluded_metadata: Vec::new(),
            column_roles: Vec::new(),
            thousands: None,
            date_order: DateOrder::default(),
            missing_tokens: Vec::new(),
            comment_prefix: String::new(),
            skip_data_rows: 0,
            encoding: Encoding::Auto,
            auto_match: AutoMatch::default(),
            column_kinds: Vec::new(),
            import_mode: ImportMode::NewSheet,
            files_layout: FilesLayout::SheetPerFile,
            block_split: BlockSplit::None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_is_permissive_by_default() {
        let recipe = ImportRecipe::new("test");
        assert_eq!(recipe.name, "test");
        assert_eq!(recipe.separator, Separator::Auto);
        assert_eq!(recipe.decimal, DecimalStyle::Dot);
        assert_eq!(recipe.header, HeaderSkip::None);
        assert_eq!(recipe.footer_skip_lines, 0);
        assert!(!recipe.has_column_header_row);
        assert_eq!(recipe.name_source, NameSource::Filename);
        assert_eq!(recipe.header_layout, HeaderLayout::default());
        assert_eq!(recipe.subheader_roles, SubheaderRoles::default());
        assert_eq!(recipe.skip_chars, 0);
        assert_eq!(recipe.preview_lines, 20);
        assert_eq!(recipe.data_end, DataEnd::None);
        assert!(recipe.keep_header_metadata);
        assert!(!recipe.keep_footer_metadata);
        assert!(recipe.excluded_metadata.is_empty());
    }

    #[test]
    fn unify_header_turns_lines_into_a_main_header() {
        let mut recipe = ImportRecipe::new("t");
        recipe.header = HeaderSkip::Lines(3);
        recipe.unify_header();
        assert_eq!(recipe.header, HeaderSkip::None);
        assert_eq!(recipe.header_layout.main_header_lines, 3);
        // The `auto_detect` flag alone (old recipes) does not count.
        let mut old = ImportRecipe::new("t");
        old.header = HeaderSkip::Lines(2);
        old.header_layout.auto_detect = true;
        old.unify_header();
        assert_eq!(old.header_layout.main_header_lines, 2);
        assert_eq!(old.header, HeaderSkip::None);
        // Already structured, or marker: nothing moves.
        recipe.header = HeaderSkip::UntilMarker("X".into());
        recipe.unify_header();
        assert_eq!(recipe.header, HeaderSkip::UntilMarker("X".into()));
    }

    #[test]
    fn column_roles_default_to_x_then_y() {
        let mut recipe = ImportRecipe::new("t");
        assert_eq!(recipe.column_role(0), ColumnRoleChoice::X);
        assert_eq!(recipe.column_role(5), ColumnRoleChoice::Y);
        recipe.column_roles = vec![ColumnRoleChoice::Ignore];
        assert_eq!(recipe.column_role(0), ColumnRoleChoice::Ignore);
        assert_eq!(recipe.column_role(1), ColumnRoleChoice::Y);
    }

    #[test]
    fn round_trip_json() {
        let recipe = ImportRecipe {
            name: "Jasco".into(),
            separator: Separator::Semicolon,
            decimal: DecimalStyle::Comma,
            header: HeaderSkip::UntilMarker("XYDATA".into()),
            footer_skip_lines: 2,
            has_column_header_row: false,
            name_source: NameSource::HeaderKey("TITLE".into()),
            header_layout: HeaderLayout {
                main_header_lines: 3,
                subheader_lines: 4,
                auto_detect: true,
            },
            subheader_roles: SubheaderRoles {
                long_name: Some(SubheaderLineRange::single(1)),
                unit: Some(SubheaderLineRange { from: 2, to: 2 }),
                comment: None,
                x_prime: None,
                same_line_name_unit: false,
            },
            skip_chars: 5,
            preview_lines: 50,
            data_end: DataEnd::Marker("##### Extended".into()),
            keep_header_metadata: false,
            keep_footer_metadata: true,
            excluded_metadata: vec!["DATE".into()],
            column_roles: vec![ColumnRoleChoice::X, ColumnRoleChoice::Ignore],
            thousands: Some(' '),
            date_order: DateOrder::MonthFirst,
            missing_tokens: vec!["--".into(), "n/a".into()],
            comment_prefix: "#".into(),
            skip_data_rows: 2,
            encoding: Encoding::Windows1252,
            auto_match: AutoMatch {
                extensions: vec!["csv".into()],
                name_contains: "jasco".into(),
                content_contains: "ORIGIN;JASCO".into(),
            },
            column_kinds: vec![ColumnKind::DateTime, ColumnKind::Text],
            import_mode: ImportMode::AddRows,
            files_layout: FilesLayout::SharedX,
            block_split: BlockSplit::BeforeLine("TITLE".into()),
        };
        let json = serde_json::to_string(&recipe).unwrap();
        assert!(json.contains("MonthFirst"));
        let back: ImportRecipe = serde_json::from_str(&json).unwrap();
        assert_eq!(recipe, back);
    }

    /// A recipe saved by `RecipeStore` **before** that work package (so
    /// without `header_layout`/`subheader_roles`/`skip_chars`/`preview_lines`
    /// in its JSON) must keep loading, with the new fields at their default
    /// values. `#[serde(default)]` is what keeps this promise; this test
    /// checks it rather than assuming it.
    #[test]
    fn json_recipe_older_than_the_work_package_loads_with_the_defaults() {
        let json_before_work_package = r#"{
            "name": "Jasco UV-Vis",
            "separator": "Semicolon",
            "decimal": "Comma",
            "header": {"Lines": 3},
            "footer_skip_lines": 2,
            "has_column_header_row": true,
            "name_source": "Filename"
        }"#;
        let recipe: ImportRecipe = serde_json::from_str(json_before_work_package)
            .expect("a JSON without the new fields must stay readable");
        assert_eq!(recipe.header_layout, HeaderLayout::default());
        assert_eq!(recipe.subheader_roles, SubheaderRoles::default());
        assert_eq!(recipe.skip_chars, 0);
        assert_eq!(recipe.preview_lines, 20);
        assert_eq!(recipe.data_end, DataEnd::None);
        assert!(recipe.keep_header_metadata, "everything is kept by default");
        assert_eq!(recipe.date_order, DateOrder::DayFirst);
    }
}

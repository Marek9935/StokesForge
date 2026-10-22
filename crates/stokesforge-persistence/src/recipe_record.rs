//! Recette d'import, telle qu'elle s'écrit dans le fichier (S2, #323).
//!
//! Chaque type de `stokesforge_import::recipe` a ici son miroir : le format
//! du fichier ne dépend plus de la forme interne de la recette, un champ ou
//! une variante renommés dans `stokesforge-import` ne changent pas les octets
//! écrits. Les champs apparus après la première recette portent un
//! `#[serde(default)]`, comme dans le type d'origine.

use serde::{Deserialize, Serialize};
use stokesforge_import::recipe as r;

fn default_true() -> bool {
    true
}

fn default_preview_lines() -> u64 {
    20
}

/// Source d'une colonne importée (`stokesforge_core::ImportSource`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct ImportSourceRecord {
    /// Chemin du fichier source.
    pub(crate) path: String,
    /// La recette qui l'a lu.
    pub(crate) recipe: ImportRecipeRecord,
    /// Index de la colonne dans le fichier (0 = la première).
    pub(crate) column: u64,
    /// Rang du bloc de données dans le fichier (0 = le premier).
    pub(crate) block: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct ImportRecipeRecord {
    pub(crate) name: String,
    pub(crate) separator: SeparatorRecord,
    pub(crate) decimal: DecimalStyleRecord,
    pub(crate) header: HeaderSkipRecord,
    pub(crate) footer_skip_lines: u64,
    pub(crate) has_column_header_row: bool,
    pub(crate) name_source: NameSourceRecord,
    #[serde(default)]
    pub(crate) header_layout: HeaderLayoutRecord,
    #[serde(default)]
    pub(crate) subheader_roles: SubheaderRolesRecord,
    #[serde(default)]
    pub(crate) skip_chars: u64,
    #[serde(default = "default_preview_lines")]
    pub(crate) preview_lines: u64,
    #[serde(default)]
    pub(crate) data_end: DataEndRecord,
    #[serde(default = "default_true")]
    pub(crate) keep_header_metadata: bool,
    #[serde(default)]
    pub(crate) keep_footer_metadata: bool,
    #[serde(default)]
    pub(crate) excluded_metadata: Vec<String>,
    #[serde(default)]
    pub(crate) column_roles: Vec<ColumnRoleChoiceRecord>,
    #[serde(default)]
    pub(crate) thousands: Option<char>,
    #[serde(default)]
    pub(crate) date_order: DateOrderRecord,
    #[serde(default)]
    pub(crate) missing_tokens: Vec<String>,
    #[serde(default)]
    pub(crate) comment_prefix: String,
    #[serde(default)]
    pub(crate) skip_data_rows: u64,
    #[serde(default)]
    pub(crate) encoding: EncodingRecord,
    #[serde(default)]
    pub(crate) auto_match: AutoMatchRecord,
    #[serde(default)]
    pub(crate) column_kinds: Vec<ColumnKindRecord>,
    #[serde(default)]
    pub(crate) import_mode: ImportModeRecord,
    #[serde(default)]
    pub(crate) files_layout: FilesLayoutRecord,
    #[serde(default)]
    pub(crate) block_split: BlockSplitRecord,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub(crate) enum BlockSplitRecord {
    #[default]
    None,
    BeforeLine(String),
    BlankLines,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub(crate) enum ImportModeRecord {
    #[default]
    NewSheet,
    ReplaceSheet,
    AddColumns,
    AddRows,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub(crate) enum FilesLayoutRecord {
    #[default]
    SheetPerFile,
    SideBySide,
    SharedX,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub(crate) enum ColumnKindRecord {
    #[default]
    Auto,
    Text,
    DateTime,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub(crate) struct AutoMatchRecord {
    #[serde(default)]
    pub(crate) extensions: Vec<String>,
    #[serde(default)]
    pub(crate) name_contains: String,
    #[serde(default)]
    pub(crate) content_contains: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub(crate) enum DateOrderRecord {
    #[default]
    DayFirst,
    MonthFirst,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub(crate) enum EncodingRecord {
    #[default]
    Auto,
    Utf8,
    Windows1252,
    Latin1,
    Utf16Le,
    Utf16Be,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub(crate) enum ColumnRoleChoiceRecord {
    #[default]
    Auto,
    X,
    Y,
    XError,
    YError,
    Ignore,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub(crate) enum DataEndRecord {
    Auto,
    #[default]
    None,
    FirstBlankLine,
    Marker(String),
    LinesAfterHeader(u64),
    LinesBeforeEnd(u64),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct SubheaderLineRangeRecord {
    pub(crate) from: u64,
    pub(crate) to: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub(crate) struct HeaderLayoutRecord {
    pub(crate) main_header_lines: u64,
    pub(crate) subheader_lines: u64,
    pub(crate) auto_detect: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum SeparatorRecord {
    Comma,
    Semicolon,
    Tab,
    Whitespace,
    Custom(char),
    FixedWidth(Vec<u64>),
    Auto,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum DecimalStyleRecord {
    Dot,
    Comma,
    Auto,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) enum HeaderSkipRecord {
    None,
    Lines(u64),
    UntilMarker(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) enum NameSourceRecord {
    Filename,
    HeaderKey(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub(crate) struct SubheaderRolesRecord {
    pub(crate) long_name: Option<SubheaderLineRangeRecord>,
    pub(crate) unit: Option<SubheaderLineRangeRecord>,
    pub(crate) comment: Option<SubheaderLineRangeRecord>,
    #[serde(default)]
    pub(crate) x_prime: Option<SubheaderLineRangeRecord>,
    pub(crate) same_line_name_unit: bool,
}

// ── Conversions ─────────────────────────────────────────────────────────
//
// Un `match` exhaustif sans `_` par énumération : une variante ajoutée dans
// `stokesforge-import` ne compile pas ici tant qu'elle n'a pas son miroir.

fn to_usize(value: u64) -> usize {
    usize::try_from(value).unwrap_or(usize::MAX)
}

macro_rules! unit_enum {
    ($record:ident, $model:ident { $($variant:ident),+ }) => {
        impl From<&r::$model> for $record {
            fn from(value: &r::$model) -> Self {
                match value {
                    $(r::$model::$variant => $record::$variant,)+
                }
            }
        }
        impl From<$record> for r::$model {
            fn from(value: $record) -> Self {
                match value {
                    $($record::$variant => r::$model::$variant,)+
                }
            }
        }
    };
}

unit_enum!(
    ImportModeRecord,
    ImportMode {
        NewSheet,
        ReplaceSheet,
        AddColumns,
        AddRows
    }
);
unit_enum!(
    FilesLayoutRecord,
    FilesLayout {
        SheetPerFile,
        SideBySide,
        SharedX
    }
);
unit_enum!(
    ColumnKindRecord,
    ColumnKind {
        Auto,
        Text,
        DateTime
    }
);
unit_enum!(
    DateOrderRecord,
    DateOrder {
        DayFirst,
        MonthFirst
    }
);
unit_enum!(
    EncodingRecord,
    Encoding {
        Auto,
        Utf8,
        Windows1252,
        Latin1,
        Utf16Le,
        Utf16Be
    }
);
unit_enum!(
    ColumnRoleChoiceRecord,
    ColumnRoleChoice {
        Auto,
        X,
        Y,
        XError,
        YError,
        Ignore
    }
);
unit_enum!(DecimalStyleRecord, DecimalStyle { Dot, Comma, Auto });

impl From<&r::BlockSplit> for BlockSplitRecord {
    fn from(value: &r::BlockSplit) -> Self {
        match value {
            r::BlockSplit::None => BlockSplitRecord::None,
            r::BlockSplit::BeforeLine(line) => BlockSplitRecord::BeforeLine(line.clone()),
            r::BlockSplit::BlankLines => BlockSplitRecord::BlankLines,
        }
    }
}

impl From<BlockSplitRecord> for r::BlockSplit {
    fn from(value: BlockSplitRecord) -> Self {
        match value {
            BlockSplitRecord::None => r::BlockSplit::None,
            BlockSplitRecord::BeforeLine(line) => r::BlockSplit::BeforeLine(line),
            BlockSplitRecord::BlankLines => r::BlockSplit::BlankLines,
        }
    }
}

impl From<&r::AutoMatch> for AutoMatchRecord {
    fn from(value: &r::AutoMatch) -> Self {
        AutoMatchRecord {
            extensions: value.extensions.clone(),
            name_contains: value.name_contains.clone(),
            content_contains: value.content_contains.clone(),
        }
    }
}

impl From<AutoMatchRecord> for r::AutoMatch {
    fn from(value: AutoMatchRecord) -> Self {
        r::AutoMatch {
            extensions: value.extensions,
            name_contains: value.name_contains,
            content_contains: value.content_contains,
        }
    }
}

impl From<&r::DataEnd> for DataEndRecord {
    fn from(value: &r::DataEnd) -> Self {
        match value {
            r::DataEnd::Auto => DataEndRecord::Auto,
            r::DataEnd::None => DataEndRecord::None,
            r::DataEnd::FirstBlankLine => DataEndRecord::FirstBlankLine,
            r::DataEnd::Marker(marker) => DataEndRecord::Marker(marker.clone()),
            r::DataEnd::LinesAfterHeader(n) => DataEndRecord::LinesAfterHeader(*n as u64),
            r::DataEnd::LinesBeforeEnd(n) => DataEndRecord::LinesBeforeEnd(*n as u64),
        }
    }
}

impl From<DataEndRecord> for r::DataEnd {
    fn from(value: DataEndRecord) -> Self {
        match value {
            DataEndRecord::Auto => r::DataEnd::Auto,
            DataEndRecord::None => r::DataEnd::None,
            DataEndRecord::FirstBlankLine => r::DataEnd::FirstBlankLine,
            DataEndRecord::Marker(marker) => r::DataEnd::Marker(marker),
            DataEndRecord::LinesAfterHeader(n) => r::DataEnd::LinesAfterHeader(to_usize(n)),
            DataEndRecord::LinesBeforeEnd(n) => r::DataEnd::LinesBeforeEnd(to_usize(n)),
        }
    }
}

impl From<&r::SubheaderLineRange> for SubheaderLineRangeRecord {
    fn from(value: &r::SubheaderLineRange) -> Self {
        SubheaderLineRangeRecord {
            from: value.from as u64,
            to: value.to as u64,
        }
    }
}

impl From<SubheaderLineRangeRecord> for r::SubheaderLineRange {
    fn from(value: SubheaderLineRangeRecord) -> Self {
        r::SubheaderLineRange {
            from: to_usize(value.from),
            to: to_usize(value.to),
        }
    }
}

impl From<&r::HeaderLayout> for HeaderLayoutRecord {
    fn from(value: &r::HeaderLayout) -> Self {
        HeaderLayoutRecord {
            main_header_lines: value.main_header_lines as u64,
            subheader_lines: value.subheader_lines as u64,
            auto_detect: value.auto_detect,
        }
    }
}

impl From<HeaderLayoutRecord> for r::HeaderLayout {
    fn from(value: HeaderLayoutRecord) -> Self {
        r::HeaderLayout {
            main_header_lines: to_usize(value.main_header_lines),
            subheader_lines: to_usize(value.subheader_lines),
            auto_detect: value.auto_detect,
        }
    }
}

impl From<&r::SubheaderRoles> for SubheaderRolesRecord {
    fn from(value: &r::SubheaderRoles) -> Self {
        SubheaderRolesRecord {
            long_name: value.long_name.as_ref().map(Into::into),
            unit: value.unit.as_ref().map(Into::into),
            comment: value.comment.as_ref().map(Into::into),
            x_prime: value.x_prime.as_ref().map(Into::into),
            same_line_name_unit: value.same_line_name_unit,
        }
    }
}

impl From<SubheaderRolesRecord> for r::SubheaderRoles {
    fn from(value: SubheaderRolesRecord) -> Self {
        r::SubheaderRoles {
            long_name: value.long_name.map(Into::into),
            unit: value.unit.map(Into::into),
            comment: value.comment.map(Into::into),
            x_prime: value.x_prime.map(Into::into),
            same_line_name_unit: value.same_line_name_unit,
        }
    }
}

impl From<&r::Separator> for SeparatorRecord {
    fn from(value: &r::Separator) -> Self {
        match value {
            r::Separator::Comma => SeparatorRecord::Comma,
            r::Separator::Semicolon => SeparatorRecord::Semicolon,
            r::Separator::Tab => SeparatorRecord::Tab,
            r::Separator::Whitespace => SeparatorRecord::Whitespace,
            r::Separator::Custom(c) => SeparatorRecord::Custom(*c),
            r::Separator::FixedWidth(widths) => {
                SeparatorRecord::FixedWidth(widths.iter().map(|w| *w as u64).collect())
            }
            r::Separator::Auto => SeparatorRecord::Auto,
        }
    }
}

impl From<SeparatorRecord> for r::Separator {
    fn from(value: SeparatorRecord) -> Self {
        match value {
            SeparatorRecord::Comma => r::Separator::Comma,
            SeparatorRecord::Semicolon => r::Separator::Semicolon,
            SeparatorRecord::Tab => r::Separator::Tab,
            SeparatorRecord::Whitespace => r::Separator::Whitespace,
            SeparatorRecord::Custom(c) => r::Separator::Custom(c),
            SeparatorRecord::FixedWidth(widths) => {
                r::Separator::FixedWidth(widths.into_iter().map(to_usize).collect())
            }
            SeparatorRecord::Auto => r::Separator::Auto,
        }
    }
}

impl From<&r::HeaderSkip> for HeaderSkipRecord {
    fn from(value: &r::HeaderSkip) -> Self {
        match value {
            r::HeaderSkip::None => HeaderSkipRecord::None,
            r::HeaderSkip::Lines(n) => HeaderSkipRecord::Lines(*n as u64),
            r::HeaderSkip::UntilMarker(marker) => HeaderSkipRecord::UntilMarker(marker.clone()),
        }
    }
}

impl From<HeaderSkipRecord> for r::HeaderSkip {
    fn from(value: HeaderSkipRecord) -> Self {
        match value {
            HeaderSkipRecord::None => r::HeaderSkip::None,
            HeaderSkipRecord::Lines(n) => r::HeaderSkip::Lines(to_usize(n)),
            HeaderSkipRecord::UntilMarker(marker) => r::HeaderSkip::UntilMarker(marker),
        }
    }
}

impl From<&r::NameSource> for NameSourceRecord {
    fn from(value: &r::NameSource) -> Self {
        match value {
            r::NameSource::Filename => NameSourceRecord::Filename,
            r::NameSource::HeaderKey(key) => NameSourceRecord::HeaderKey(key.clone()),
        }
    }
}

impl From<NameSourceRecord> for r::NameSource {
    fn from(value: NameSourceRecord) -> Self {
        match value {
            NameSourceRecord::Filename => r::NameSource::Filename,
            NameSourceRecord::HeaderKey(key) => r::NameSource::HeaderKey(key),
        }
    }
}

impl From<&r::ImportRecipe> for ImportRecipeRecord {
    fn from(value: &r::ImportRecipe) -> Self {
        ImportRecipeRecord {
            name: value.name.clone(),
            separator: (&value.separator).into(),
            decimal: (&value.decimal).into(),
            header: (&value.header).into(),
            footer_skip_lines: value.footer_skip_lines as u64,
            has_column_header_row: value.has_column_header_row,
            name_source: (&value.name_source).into(),
            header_layout: (&value.header_layout).into(),
            subheader_roles: (&value.subheader_roles).into(),
            skip_chars: value.skip_chars as u64,
            preview_lines: value.preview_lines as u64,
            data_end: (&value.data_end).into(),
            keep_header_metadata: value.keep_header_metadata,
            keep_footer_metadata: value.keep_footer_metadata,
            excluded_metadata: value.excluded_metadata.clone(),
            column_roles: value.column_roles.iter().map(Into::into).collect(),
            thousands: value.thousands,
            date_order: (&value.date_order).into(),
            missing_tokens: value.missing_tokens.clone(),
            comment_prefix: value.comment_prefix.clone(),
            skip_data_rows: value.skip_data_rows as u64,
            encoding: (&value.encoding).into(),
            auto_match: (&value.auto_match).into(),
            column_kinds: value.column_kinds.iter().map(Into::into).collect(),
            import_mode: (&value.import_mode).into(),
            files_layout: (&value.files_layout).into(),
            block_split: (&value.block_split).into(),
        }
    }
}

impl From<ImportRecipeRecord> for r::ImportRecipe {
    fn from(value: ImportRecipeRecord) -> Self {
        r::ImportRecipe {
            name: value.name,
            separator: value.separator.into(),
            decimal: value.decimal.into(),
            header: value.header.into(),
            footer_skip_lines: to_usize(value.footer_skip_lines),
            has_column_header_row: value.has_column_header_row,
            name_source: value.name_source.into(),
            header_layout: value.header_layout.into(),
            subheader_roles: value.subheader_roles.into(),
            skip_chars: to_usize(value.skip_chars),
            preview_lines: to_usize(value.preview_lines),
            data_end: value.data_end.into(),
            keep_header_metadata: value.keep_header_metadata,
            keep_footer_metadata: value.keep_footer_metadata,
            excluded_metadata: value.excluded_metadata,
            column_roles: value.column_roles.into_iter().map(Into::into).collect(),
            thousands: value.thousands,
            date_order: value.date_order.into(),
            missing_tokens: value.missing_tokens,
            comment_prefix: value.comment_prefix,
            skip_data_rows: to_usize(value.skip_data_rows),
            encoding: value.encoding.into(),
            auto_match: value.auto_match.into(),
            column_kinds: value.column_kinds.into_iter().map(Into::into).collect(),
            import_mode: value.import_mode.into(),
            files_layout: value.files_layout.into(),
            block_split: value.block_split.into(),
        }
    }
}

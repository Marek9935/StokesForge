//! Legend of a panel.

use crate::text_anchor::TextAnchor;

/// Corner of the panel where the legend is anchored.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum LegendPosition {
    /// Top left.
    TopLeft,
    /// Top right (default).
    #[default]
    TopRight,
    /// Bottom left.
    BottomLeft,
    /// Bottom right.
    BottomRight,
}

/// Legend of a panel: visible or not, and where.
#[derive(Debug, Clone, PartialEq)]
pub struct LegendSpec {
    /// Legend is displayed.
    pub visible: bool,
    /// Anchor corner.
    pub position: LegendPosition,
    /// Position of the frame (its center) if it was moved (issue #53);
    /// takes precedence over [`Self::position`]. `None` = the anchor corner.
    pub anchor: Option<TextAnchor>,
    /// Font and size of the series names in the legend (issue #77).
    pub font: crate::TextFont,
    /// Color of the series names in the legend (issue #90); empty =
    /// automatic, like [`crate::PlotSpec::title_color`].
    pub color: String,
    /// Which worksheet header row gives the text of each entry (#499).
    pub text: LegendText,
}

impl Default for LegendSpec {
    fn default() -> Self {
        Self {
            visible: true,
            position: LegendPosition::TopRight,
            anchor: None,
            font: crate::TextFont::default(),
            color: String::new(),
            text: LegendText::default(),
        }
    }
}

/// Which header row of the worksheet gives the text of the legend entries
/// (issue #499, « Text from »).
///
/// One setting per legend, followed by every series of the panel. A typed
/// series name ([`crate::SeriesSpec::name`]) still wins over it.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub enum LegendTextFrom {
    /// The long name; with no long name, the short name. Today's behavior.
    #[default]
    LongName,
    /// The comments row.
    Comments,
    /// The x' row.
    XPrime,
    /// The units row.
    Units,
    /// The short name (the column's name).
    ShortName,
    /// « Long Name (Units) ».
    LongNameUnits,
    /// « Long Name, Comments ».
    LongNameComments,
    /// The free template [`LegendText::template`].
    Custom,
    /// A choice written by a newer version: read as [`Self::LongName`], kept
    /// as is when the file is saved again. Holds its stored key.
    Unknown(String),
}

impl LegendTextFrom {
    /// The choices of the interface, in display order.
    pub const CHOICES: [Self; 8] = [
        Self::LongName,
        Self::Comments,
        Self::XPrime,
        Self::Units,
        Self::ShortName,
        Self::LongNameUnits,
        Self::LongNameComments,
        Self::Custom,
    ];

    /// Stable key of the stored form (`.stf`, `pyst`).
    pub fn key(&self) -> &str {
        match self {
            Self::LongName => "long_name",
            Self::Comments => "comments",
            Self::XPrime => "x_prime",
            Self::Units => "units",
            Self::ShortName => "short_name",
            Self::LongNameUnits => "long_name_units",
            Self::LongNameComments => "long_name_comments",
            Self::Custom => "custom",
            Self::Unknown(key) => key,
        }
    }

    /// Choice of a stored key; an empty key is the default, an unknown one
    /// is kept as [`Self::Unknown`].
    pub fn from_key(key: &str) -> Self {
        match key {
            "" | "long_name" => Self::LongName,
            "comments" => Self::Comments,
            "x_prime" => Self::XPrime,
            "units" => Self::Units,
            "short_name" => Self::ShortName,
            "long_name_units" => Self::LongNameUnits,
            "long_name_comments" => Self::LongNameComments,
            "custom" => Self::Custom,
            other => Self::Unknown(other.to_string()),
        }
    }
}

/// The legend-level « Text from » setting (issue #499).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LegendText {
    /// The header row the entries read.
    pub from: LegendTextFrom,
    /// Template of [`LegendTextFrom::Custom`]: Typst markup in which the
    /// placeholders of [`PLACEHOLDERS`] insert the column text, escaped.
    pub template: String,
    /// For a row series (#166): the column whose cell on that row gives the
    /// entry. `None` = « Row N ».
    pub row_column: Option<stokesforge_id::NodeId>,
}

impl Default for LegendText {
    fn default() -> Self {
        Self {
            from: LegendTextFrom::LongName,
            template: String::new(),
            row_column: None,
        }
    }
}

/// A piece of column text a template placeholder can insert.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LegendField {
    /// Long name.
    LongName,
    /// Units.
    Units,
    /// Comments.
    Comments,
    /// Short name.
    ShortName,
    /// The x' value.
    XPrime,
}

/// The placeholders of a custom template, with their meaning. **The one
/// place** that defines the syntax (`%(LL)`…): `@` is a Typst reference, so
/// it cannot introduce a placeholder.
pub const PLACEHOLDERS: [(&str, LegendField); 5] = [
    ("%(LL)", LegendField::LongName),
    ("%(LU)", LegendField::Units),
    ("%(LC)", LegendField::Comments),
    ("%(LS)", LegendField::ShortName),
    ("%(X)", LegendField::XPrime),
];

/// Any text in a form that Typst displays **as is**: a Typst string,
/// `#"…"`, of which only `\` and `"` (and line ends) are escaped (issue #33).
pub fn typst_literal(text: &str) -> String {
    if text.is_empty() {
        return String::new();
    }
    let mut escaped = String::with_capacity(text.len() + 3);
    escaped.push_str("#\"");
    for character in text.chars() {
        match character {
            '\\' => escaped.push_str("\\\\"),
            '"' => escaped.push_str("\\\""),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            other => escaped.push(other),
        }
    }
    escaped.push('"');
    escaped
}

/// Expands a custom template: every known placeholder is replaced by the
/// column text **escaped** ([`typst_literal`]), so it is drawn literally;
/// the rest of the template is Typst markup and an unknown placeholder stays
/// as typed.
pub fn expand_template(template: &str, value: impl FnMut(LegendField) -> String) -> String {
    expand(template, value, true)
}

/// Expands a custom template for a **list** (Object Manager, series list):
/// the placeholders insert the column text as is, the rest of the template
/// stays as typed, like a typed series name does in a list.
pub fn expand_template_plain(template: &str, value: impl FnMut(LegendField) -> String) -> String {
    expand(template, value, false)
}

fn expand(template: &str, mut value: impl FnMut(LegendField) -> String, escape: bool) -> String {
    let mut out = String::with_capacity(template.len());
    let mut rest = template;
    'scan: while !rest.is_empty() {
        for (token, field) in PLACEHOLDERS {
            if let Some(after) = rest.strip_prefix(token) {
                let text = value(field);
                if !escape {
                    out.push_str(&text);
                    rest = after;
                    continue 'scan;
                }
                out.push_str(&typst_literal(&text));
                // A string followed by `.`, `(` or `[` would read as a field
                // access or a call: an empty comment separates them. (An
                // empty content block `#[]` does not: it takes the `.`, `(`
                // or `[` as its own trailer.)
                if !text.is_empty() && after.starts_with(['.', '(', '[']) {
                    out.push_str("/**/");
                }
                rest = after;
                continue 'scan;
            }
        }
        let mut chars = rest.chars();
        if let Some(c) = chars.next() {
            out.push(c);
        }
        rest = chars.as_str();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn value(field: LegendField) -> String {
        match field {
            LegendField::LongName => "A *b* #c".to_string(),
            LegendField::Units => "mM".to_string(),
            LegendField::Comments => String::new(),
            LegendField::ShortName => "B".to_string(),
            LegendField::XPrime => "57".to_string(),
        }
    }

    #[test]
    fn a_placeholder_inserts_the_escaped_text_and_the_rest_is_markup() {
        assert_eq!(
            expand_template("_%(LL)_ [%(LU)]", value),
            "_#\"A *b* #c\"_ [#\"mM\"]"
        );
    }

    #[test]
    fn the_plain_expansion_keeps_the_column_text_unescaped_for_lists() {
        assert_eq!(
            expand_template_plain("_%(LL)_ [%(LU)]%(LS).x", value),
            "_A *b* #c_ [mM]B.x"
        );
    }

    #[test]
    fn an_unknown_placeholder_and_an_empty_value_are_left_alone() {
        assert_eq!(expand_template("%(QQ) %(LC)|%(X)", value), "%(QQ) |#\"57\"");
    }

    #[test]
    fn a_string_followed_by_a_dot_or_a_bracket_is_not_read_as_code() {
        assert_eq!(expand_template("%(LS).x", value), "#\"B\"/**/.x");
        assert_eq!(expand_template("%(LS)(1)", value), "#\"B\"/**/(1)");
        assert_eq!(expand_template("%(LS) .", value), "#\"B\" .");
    }

    #[test]
    fn a_quote_or_backslash_in_the_value_is_escaped() {
        assert_eq!(typst_literal("a\"b\\c"), "#\"a\\\"b\\\\c\"");
        assert_eq!(typst_literal(""), "");
    }

    #[test]
    fn the_keys_round_trip_and_an_unknown_key_is_kept() {
        for from in LegendTextFrom::CHOICES {
            assert_eq!(LegendTextFrom::from_key(from.key()), from);
        }
        assert_eq!(LegendTextFrom::from_key(""), LegendTextFrom::LongName);
        let future = LegendTextFrom::from_key("from_the_future");
        assert_eq!(future.key(), "from_the_future");
    }
}

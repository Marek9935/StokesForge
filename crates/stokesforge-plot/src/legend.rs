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
    /// Template of [`LegendTextFrom::Custom`]: Typst markup that reads the
    /// variables of [`VARIABLES`] (`#long-name`, `#ln`, `#units`…).
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

/// A piece of column text a template variable holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LegendField {
    /// Long name.
    LongName,
    /// Short name.
    ShortName,
    /// Units.
    Units,
    /// Comments.
    Comments,
    /// The x' value.
    XPrime,
}

/// The variables of a custom template: long name, short name, field. **The
/// one place** that defines them; the names are the same in every interface
/// language, so a template saved in a file works whatever the language.
pub const VARIABLES: [(&str, &str, LegendField); 5] = [
    ("long-name", "ln", LegendField::LongName),
    ("short-name", "sn", LegendField::ShortName),
    ("units", "u", LegendField::Units),
    ("comments", "c", LegendField::Comments),
    ("x-prime", "xp", LegendField::XPrime),
];

/// Any text as a Typst **string literal** (`"…"`): only `\` and `"` (and line
/// ends) are escaped, so the text is drawn as is whatever characters it holds.
pub fn typst_string(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len() + 2);
    escaped.push('"');
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

/// Any text in a form that Typst displays **as is** in markup: a Typst
/// string, `#"…"`. Empty text gives nothing (issue #33).
pub fn typst_literal(text: &str) -> String {
    if text.is_empty() {
        String::new()
    } else {
        format!("#{}", typst_string(text))
    }
}

/// The Typst source of a custom template for one legend entry: a preamble
/// that defines every variable of [`VARIABLES`] (long and short name) as a
/// string of the entry's column, then the template as typed. `value` gives
/// the text of each field.
pub fn template_source(template: &str, mut value: impl FnMut(LegendField) -> String) -> String {
    let mut source = String::new();
    for (long, short, field) in VARIABLES {
        let quoted = typst_string(&value(field));
        source.push_str(&format!("#let {long} = {quoted}\n#let {short} = {long}\n"));
    }
    source.push_str(template);
    source
}

impl LegendTextFrom {
    /// The custom template that gives the same text as this choice: what
    /// « Custom… » starts from. `None` for [`Self::Custom`] itself.
    pub fn template(&self) -> Option<&'static str> {
        Some(match self {
            Self::LongName | Self::Unknown(_) => {
                "#if long-name != \"\" [#long-name] else [#short-name]"
            }
            Self::Comments => "#comments",
            Self::XPrime => "#x-prime",
            Self::Units => "#units",
            Self::ShortName => "#short-name",
            Self::LongNameUnits => "#long-name #if units != \"\" [(#units)]",
            Self::LongNameComments => "#long-name#if comments != \"\" [, #comments]",
            Self::Custom => return None,
        })
    }
}

impl LegendText {
    /// Chooses [`LegendTextFrom::Custom`] and, when the template is empty or
    /// still one of the templates of the choices, starts it from the one of
    /// the previous choice. What the user typed is never overwritten.
    pub fn choose_custom(&mut self) {
        let previous = self.from.template();
        let untouched = self.template.is_empty()
            || LegendTextFrom::CHOICES
                .iter()
                .any(|from| from.template() == Some(self.template.as_str()));
        if let (true, Some(previous)) = (untouched, previous) {
            self.template = previous.to_string();
        }
        self.from = LegendTextFrom::Custom;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn value(field: LegendField) -> String {
        match field {
            LegendField::LongName => "A \"*b*\" #c $d \\".to_string(),
            LegendField::Units => "mM".to_string(),
            LegendField::Comments => String::new(),
            LegendField::ShortName => "B".to_string(),
            LegendField::XPrime => "57".to_string(),
        }
    }

    /// Witness: no escape of `"` or `\` would end the string early.
    #[test]
    fn the_variables_are_string_literals_that_hold_any_character() {
        let source = template_source("#ln", value);
        assert!(source.contains("#let long-name = \"A \\\"*b*\\\" #c $d \\\\\"\n"));
        assert!(source.contains("#let ln = long-name\n"));
        assert!(source.contains("#let u = units\n"));
        assert!(source.ends_with("#ln"));
        assert_eq!(typst_literal(""), "");
        assert_eq!(typst_literal("a\"b\\c"), "#\"a\\\"b\\\\c\"");
    }

    /// Witness: prefilling over a typed template, or not prefilling an empty
    /// one, fails here.
    #[test]
    fn custom_starts_from_the_previous_choice_and_keeps_what_was_typed() {
        for from in LegendTextFrom::CHOICES {
            let Some(template) = from.template() else {
                continue;
            };
            let mut text = LegendText {
                from: from.clone(),
                ..LegendText::default()
            };
            text.choose_custom();
            assert_eq!(text.from, LegendTextFrom::Custom);
            assert_eq!(text.template, template, "{from:?}");
        }
        // Typed template: kept.
        let mut text = LegendText {
            from: LegendTextFrom::Comments,
            template: "*#ln*".to_string(),
            ..LegendText::default()
        };
        text.choose_custom();
        assert_eq!(text.template, "*#ln*");
        // A previous prefill is replaced by the new choice's one.
        let mut text = LegendText {
            from: LegendTextFrom::Units,
            template: LegendTextFrom::Comments.template().unwrap().to_string(),
            ..LegendText::default()
        };
        text.choose_custom();
        assert_eq!(text.template, "#units");
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

//! Column variables in any graph text (#501): `#ln`, `#u`… resolve like in the
//! legend Custom template of #499.
//!
//! [`bind`] adds, in front of a text, one `#let` per variable the text uses as
//! **code** (`#ln`, `#if u != "" […]`, `#units`). A math identifier (`$ln(x)$`)
//! is never a variable: the logarithm stays the logarithm. A text that uses no
//! variable comes back byte-identical, so it keeps its cache entry and its
//! pixels.
//!
//! The preamble is on a single line, framed by two comments that draw nothing,
//! so it adds no line and no space to a title. A text that starts with a
//! markup line marker (`- `, `= `…) gets a newline after it instead, so the
//! marker still starts a line. [`strip_preamble`] drops it
//! again: the `epaint` fallback of invariant 10 shows and measures what the
//! user typed, never the `#let`s.

use std::borrow::Cow;

use stokesforge_core::resolve::{column_fields, text_context, TextTarget};
use stokesforge_core::Tree;
use stokesforge_plot::{typst_string, AxisSpec, LegendField, PlotSpec};
use typst::syntax::{parse, SyntaxKind, SyntaxNode};

use crate::texts::FigureTexts;

/// The variables: long name, short name, field. The same table as the legend
/// Custom template (`stokesforge_plot::VARIABLES`), the one place that defines
/// them.
pub const VARIABLES: [(&str, &str, LegendField); 5] = stokesforge_plot::VARIABLES;

/// What each variable stands for: the text of the column's fields.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct VariableValues {
    pub long_name: String,
    pub short_name: String,
    pub units: String,
    pub comments: String,
    pub x_prime: String,
}

impl VariableValues {
    /// The text of one field.
    pub fn get(&self, field: LegendField) -> &str {
        match field {
            LegendField::LongName => &self.long_name,
            LegendField::ShortName => &self.short_name,
            LegendField::Units => &self.units,
            LegendField::Comments => &self.comments,
            LegendField::XPrime => &self.x_prime,
        }
    }
}

/// Opens the preamble. A comment: it draws nothing.
const START: &str = "/*sf-vars*/";
/// Closes the preamble.
const END: &str = "/*sf-end*/";
/// Closes the preamble of a text that starts with a line marker: the text then
/// starts on its own line, so the marker keeps its meaning.
const END_LINE: &str = "/*sf-end-nl*/\n";

/// True if `text` starts with a markup line marker (`= `, `- `, `+ `, `/ `,
/// `1. `), which only means something at the start of a line.
fn starts_with_line_marker(text: &str) -> bool {
    let rest = text.trim_start_matches(['=', ' ', '\t']);
    let heading = text.starts_with('=') && rest.len() < text.len();
    let digits = text.trim_start_matches(|c: char| c.is_ascii_digit());
    let numbered = digits.len() < text.len() && digits.starts_with(". ");
    let bullet =
        matches!(text.chars().next(), Some('-' | '+' | '/')) && text[1..].starts_with([' ', '\t']);
    heading || numbered || bullet
}

fn collect_identifiers(node: &SyntaxNode, used: &mut Vec<String>) {
    if node.kind() == SyntaxKind::Ident {
        if let Some(ident) = node.cast::<typst::syntax::ast::Ident>() {
            used.push(ident.as_str().to_owned());
        }
    }
    for child in node.children() {
        collect_identifiers(child, used);
    }
}

/// The variable names `text` uses as code. A name can only be used if the text
/// mentions it: the parse is skipped otherwise.
fn used_variables(text: &str) -> Vec<String> {
    let mut used = Vec::new();
    if VARIABLES
        .iter()
        .any(|(long, short, _)| text.contains(long) || text.contains(short))
    {
        collect_identifiers(&parse(text), &mut used);
        used.retain(|ident| {
            VARIABLES
                .iter()
                .any(|(long, short, _)| ident == long || ident == short)
        });
    }
    used
}

/// `text` with a preamble that defines the variables it uses as code. A text
/// that uses none is returned unchanged. A `#let` of the user in the text
/// comes after the preamble and wins.
pub fn bind(text: &str, values: &VariableValues) -> String {
    let used = used_variables(text);
    let mut preamble = String::new();
    for (long, short, field) in VARIABLES {
        let quoted = typst_string(values.get(field));
        for name in [long, short] {
            if used.iter().any(|ident| ident == name) {
                preamble.push_str(&format!("#let {name} = {quoted};"));
            }
        }
    }
    if preamble.is_empty() {
        return text.to_owned();
    }
    let end = if starts_with_line_marker(text) {
        END_LINE
    } else {
        END
    };
    format!("{START}{preamble}{end}{text}")
}

/// True if `text` uses a variable as code (a math identifier is none).
fn mentions(text: &str) -> bool {
    !used_variables(text).is_empty()
}

/// True if any Typst text of `spec` may use a variable.
fn any_mention(spec: &PlotSpec) -> bool {
    let axis = |axis: &AxisSpec| mentions(&axis.label) || mentions(&axis.unit);
    mentions(&spec.title)
        || spec.panels.iter().any(|panel| {
            mentions(&panel.title)
                || [&panel.bottom, &panel.top, &panel.left, &panel.right]
                    .into_iter()
                    .any(axis)
                || panel
                    .annotations
                    .iter()
                    .any(|annotation| mentions(&annotation.text))
                || panel.color_bar.as_ref().is_some_and(|bar| {
                    mentions(&bar.title)
                        || bar.segments.iter().any(|segment| mentions(&segment.label))
                })
        })
}

/// `spec` with the variables of every Typst text bound (#501): the graph and
/// layer titles, the axis titles (label and unit composed first, the clone
/// keeps the result as the label with an empty unit), the free annotations,
/// the colour-bar title and its break labels. Each text reads the column
/// [`text_context`] gives it; a reference with no column makes every variable
/// the translated « broken reference » of `texts`, like the legend.
///
/// **Every entry point calls this before** `resolve_spec` and `figure_layout`:
/// the size is then measured on what is drawn, so screen and export agree. The
/// stored spec is never changed; a spec whose texts use no variable comes back
/// borrowed, so its cache entry and pixels are untouched.
pub fn bind_spec<'a>(tree: &Tree, spec: &'a PlotSpec, texts: &FigureTexts) -> Cow<'a, PlotSpec> {
    if !any_mention(spec) {
        return Cow::Borrowed(spec);
    }
    let values = |target: TextTarget| match text_context(tree, spec, target)
        .and_then(|id| column_fields(tree, id))
    {
        Some(fields) => VariableValues {
            long_name: fields.long_name,
            short_name: fields.short_name,
            units: fields.units,
            comments: fields.comments,
            x_prime: fields.x_prime,
        },
        None => {
            let broken = texts.broken_reference.clone();
            VariableValues {
                long_name: broken.clone(),
                short_name: broken.clone(),
                units: broken.clone(),
                comments: broken.clone(),
                x_prime: broken,
            }
        }
    };
    let bound = |text: &str, target: TextTarget| -> Option<String> {
        mentions(text).then(|| bind(text, &values(target)))
    };
    let mut out = spec.clone();
    if let Some(text) = bound(&spec.title, TextTarget::Graph) {
        out.title = text;
    }
    for (index, (panel, source)) in out.panels.iter_mut().zip(spec.panels.iter()).enumerate() {
        if let Some(text) = bound(&source.title, TextTarget::Layer(index)) {
            panel.title = text;
        }
        let axes = [
            (
                &mut panel.bottom,
                &source.bottom,
                TextTarget::BottomAxis(index),
            ),
            (&mut panel.top, &source.top, TextTarget::TopAxis(index)),
            (&mut panel.left, &source.left, TextTarget::LeftAxis(index)),
            (
                &mut panel.right,
                &source.right,
                TextTarget::RightAxis(index),
            ),
        ];
        for (axis, original, target) in axes {
            if let Some(text) = bound(&original.display_label(), target) {
                axis.label = text;
                axis.unit.clear();
            }
        }
        for (annotation, original) in panel.annotations.iter_mut().zip(&source.annotations) {
            if let Some(text) = bound(&original.text, TextTarget::Layer(index)) {
                annotation.text = text;
            }
        }
        if let (Some(bar), Some(original)) = (panel.color_bar.as_mut(), source.color_bar.as_ref()) {
            if let Some(text) = bound(&original.title, TextTarget::ColorBar(index)) {
                bar.title = text;
            }
            for (segment, original) in bar.segments.iter_mut().zip(&original.segments) {
                if let Some(text) = bound(&original.label, TextTarget::Group(index, segment.group))
                {
                    segment.label = text;
                }
            }
        }
    }
    Cow::Owned(out)
}

/// `text` without the preamble [`bind`] put in front of it: what the user
/// typed. Identity on any other text.
pub fn strip_preamble(text: &str) -> &str {
    let Some(mut rest) = text.strip_prefix(START) else {
        return text;
    };
    loop {
        if let Some(after) = rest.strip_prefix(END_LINE) {
            return after;
        }
        if let Some(after) = rest.strip_prefix(END) {
            return after;
        }
        // `#let name = "…";` — the string may hold anything, `;` and the end
        // marker included: walk it with its escapes.
        let Some(body) = rest.strip_prefix("#let ") else {
            return text;
        };
        let Some(open) = body.find('"') else {
            return text;
        };
        let mut chars = body[open + 1..].char_indices();
        let mut close = None;
        while let Some((index, character)) = chars.next() {
            match character {
                '\\' => {
                    chars.next();
                }
                '"' => {
                    close = Some(open + 1 + index + 1);
                    break;
                }
                _ => {}
            }
        }
        let Some(close) = close else {
            return text;
        };
        match body[close..].strip_prefix(';') {
            Some(next) => rest = next,
            None => return text,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fonts::FontChoice;
    use crate::formula::{compile, fallback_text, plain_text, resolve_fonts};

    fn values(long: &str, units: &str) -> VariableValues {
        VariableValues {
            long_name: long.to_owned(),
            short_name: "s".to_owned(),
            units: units.to_owned(),
            ..Default::default()
        }
    }

    #[test]
    fn a_value_is_drawn_literally() {
        let long = "A *b* \" \\ #";
        let bound = bind("#ln", &values(long, ""));
        assert_eq!(plain_text(&bound).as_deref(), Some(long));
    }

    #[test]
    fn a_math_identifier_is_not_a_variable() {
        let text = "$ln(x)$";
        assert_eq!(bind(text, &values("Abs", "mM")), text);
    }

    #[test]
    fn a_text_without_variable_is_unchanged() {
        for text in ["Wavenumber", "", "#strong[Abs] (cm)", "a # b"] {
            assert_eq!(bind(text, &values("Abs", "mM")), text);
        }
    }

    #[test]
    fn an_axis_title_composes_without_extra_space() {
        let bound = bind("#ln#if u != \"\" [ (#u)]", &values("Abs", "mM"));
        assert!(!bound.contains('\n'), "{bound:?}");
        assert_eq!(plain_text(&bound).as_deref(), Some("Abs (mM)"));
    }

    /// Witness: the old same-line preamble draws « - Abs » as plain text, the
    /// minus included, instead of a list item.
    #[test]
    fn a_line_marker_keeps_its_meaning() {
        let values = values("Abs", "");
        for (typed, same) in [
            ("- #ln", "\n- Abs"),
            ("= #ln", "\n= Abs"),
            ("+ #ln", "\n+ Abs"),
        ] {
            let bound = bind(typed, &values);
            assert_ne!(plain_text(&bound), plain_text(&same.replace('\n', "")));
            assert_eq!(plain_text(&bound), plain_text(same), "{typed}");
            assert_eq!(strip_preamble(&bound), typed);
        }
        // A dash that is no marker stays on the same line.
        assert!(!bind("-#ln", &values).contains('\n'));
        assert!(!bind("a - #ln", &values).contains('\n'));
    }

    fn sample_with_title(title: &str) -> crate::sample::Sample {
        let mut sample = crate::sample::minimal().expect("sample");
        sample.spec.title = title.to_owned();
        sample
    }

    /// Test 6: a spec whose texts use no variable comes back borrowed, so its
    /// cache entry and pixels are untouched. Witness: always cloning.
    #[test]
    fn a_spec_without_variable_is_borrowed() {
        let sample = sample_with_title("Absorption spectrum (u.a.)");
        let bound = bind_spec(sample.project.tree(), &sample.spec, &FigureTexts::default());
        assert!(matches!(bound, Cow::Borrowed(_)));
        let math = sample_with_title("$ln(x)$ #strong[u]");
        let bound = bind_spec(math.project.tree(), &math.spec, &FigureTexts::default());
        assert!(matches!(bound, Cow::Borrowed(_)), "a math identifier");
    }

    #[test]
    fn every_text_kind_is_bound_to_its_context_column() {
        use stokesforge_plot::{AnnotationSpec, ColorBarSegment, ColorBarSpec};
        let mut sample = crate::sample::minimal().expect("sample");
        sample.spec.title = "#sn".to_owned();
        let panel = sample.spec.panels.get_mut(0).expect("panel");
        panel.title = "#sn".to_owned();
        panel.bottom.label = "#sn".to_owned();
        panel.bottom.unit = "#sn".to_owned();
        panel.annotations.push(AnnotationSpec::new("#sn", 0.0, 0.0));
        panel.series[0].group = Some(1);
        let mut bar = ColorBarSpec::default();
        bar.title = "#sn".to_owned();
        let mut segment = ColorBarSegment::new(1);
        segment.label = "#sn".to_owned();
        bar.segments.push(segment);
        panel.color_bar = Some(bar);
        let spec = sample.spec.clone();
        let bound = bind_spec(sample.project.tree(), &spec, &FigureTexts::default());
        let panel = &bound.panels[0];
        let preamble = |name: &str| format!("#let sn = \"{name}\";");
        // Titles, annotations and the bar: the Y column; the bottom axis: X.
        for text in [
            &bound.title,
            &panel.title,
            &panel.annotations[0].text,
            &panel.color_bar.as_ref().unwrap().title,
            &panel.color_bar.as_ref().unwrap().segments[0].label,
        ] {
            assert!(text.contains(&preamble("Absorbance")), "{text}");
        }
        assert!(panel.bottom.label.contains(&preamble("Lambda")));
        assert!(panel.bottom.unit.is_empty(), "the unit is composed away");
        assert_eq!(strip_preamble(&panel.bottom.label), "#sn (#sn)");
        // The stored spec is untouched.
        assert_eq!(spec.title, "#sn");
    }

    /// Test 4 (render side): no column, no series: every variable is the
    /// translated broken reference, never an empty text.
    #[test]
    fn a_text_without_a_column_reads_the_broken_reference() {
        let mut sample = sample_with_title("#ln|#u|#c|#sn|#xp");
        sample.spec.panels.get_mut(0).unwrap().series.clear();
        let texts = FigureTexts::new(
            "xx",
            "(rompue)",
            |name| name.to_owned(),
            |count| count.to_string(),
            |row| row.to_string(),
        );
        let bound = bind_spec(sample.project.tree(), &sample.spec, &texts);
        let shown = plain_text(&bound.title).expect("compiles");
        assert_eq!(shown, "(rompue)|(rompue)|(rompue)|(rompue)|(rompue)");
    }

    #[test]
    fn the_users_own_let_wins() {
        let bound = bind("#let ln = \"mine\";#ln", &values("Abs", ""));
        assert_eq!(plain_text(&bound).as_deref(), Some("mine"));
    }

    #[test]
    fn a_failing_text_falls_back_to_what_was_typed() {
        let bound = bind("#ln #nope", &values("Abs", ""));
        assert!(bound.starts_with(START));
        assert_eq!(plain_text(&bound), None);
        assert_eq!(fallback_text(&bound), "#ln #nope");
        assert_eq!(fallback_text("plain"), "plain");
    }

    #[test]
    fn a_hostile_value_binds_and_compiles() {
        let hostile = format!("{}\"\\\n*/]\u{202e}{END};#let x = \"", "é".repeat(5_000));
        assert!(hostile.len() > 10_000);
        let bound = bind("#ln #u", &values(&hostile, &hostile));
        let (sans, math) = resolve_fonts(&FontChoice::default()).unwrap();
        compile(&bound, 12.0, [0, 0, 0, 255], sans, math).expect("compiles");
        assert_eq!(strip_preamble(&bound), "#ln #u");
    }
}

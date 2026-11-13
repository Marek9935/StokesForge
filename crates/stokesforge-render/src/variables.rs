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
//! so it adds no line and no space to a title. [`strip_preamble`] drops it
//! again: the `epaint` fallback of invariant 10 shows and measures what the
//! user typed, never the `#let`s.

use std::borrow::Cow;

use stokesforge_core::resolve::{column_fields, text_context, ColumnFields, TextTarget};
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
    /// The columns of the worksheet, for `#col("C")` (#501); each entry's
    /// `short_name` is the key. Empty when the text does not ask for it.
    pub sheet: Vec<VariableValues>,
    /// What every field of an unknown column reads (`#col("Z").ln`): the
    /// translated « broken reference », or empty where there is none (the
    /// diagnosis).
    pub unknown: String,
}

impl VariableValues {
    /// Every field reads `text`: the values of a reference with no column.
    pub fn all(text: &str) -> Self {
        Self {
            long_name: text.to_owned(),
            short_name: text.to_owned(),
            units: text.to_owned(),
            comments: text.to_owned(),
            x_prime: text.to_owned(),
            ..Self::default()
        }
    }

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

impl From<&ColumnFields> for VariableValues {
    fn from(fields: &ColumnFields) -> Self {
        Self {
            long_name: fields.long_name.clone(),
            short_name: fields.short_name.clone(),
            units: fields.units.clone(),
            comments: fields.comments.clone(),
            x_prime: fields.x_prime.clone(),
            sheet: fields.sheet.iter().map(Self::from).collect(),
            unknown: String::new(),
        }
    }
}

/// The Typst error of a text typed in a field, `None` if it compiles (#501).
/// The variables are defined (empty) first, so `#ln` is valid and `#nope` is
/// not; a math identifier (`$ln(x)$`) is the logarithm, as when drawn.
pub fn diagnose_text(text: &str) -> Option<String> {
    crate::formula::diagnose(&bind(text, &VariableValues::default()))
}

/// Opens the preamble. A comment: it draws nothing.
const START: &str = "/*sf-vars*/";
/// Closes the preamble.
const END: &str = "/*sf-end*/";

/// What a text uses as code: the variable names, and `col(…)` (#501).
#[derive(Debug, Default)]
struct Used {
    names: Vec<String>,
    /// `col` is used.
    col: bool,
    /// The string literals passed to `col(…)`.
    literals: Vec<String>,
    /// `col` is called with something else than a literal (or not called):
    /// the whole sheet is then at hand.
    any_column: bool,
}

fn collect_identifiers(node: &SyntaxNode, used: &mut Used) {
    use typst::syntax::ast;
    match node.kind() {
        // `x.ln`: the field is no variable.
        SyntaxKind::FieldAccess => {
            if let Some(target) = node.children().next() {
                collect_identifiers(target, used);
            }
            return;
        }
        SyntaxKind::FuncCall
            if node
                .children()
                .next()
                .and_then(|callee| callee.cast::<ast::Ident>())
                .is_some_and(|ident| ident.as_str() == COL) =>
        {
            used.col = true;
            let args = node
                .children()
                .find(|child| child.kind() == SyntaxKind::Args);
            let first = args.and_then(|args| {
                args.children().find(|child| {
                    !matches!(
                        child.kind(),
                        SyntaxKind::LeftParen | SyntaxKind::Space | SyntaxKind::Comma
                    )
                })
            });
            match first.and_then(|node| node.cast::<ast::Str>()) {
                Some(literal) => used.literals.push(literal.get().to_string()),
                None => used.any_column = true,
            }
            for child in node.children().skip(1) {
                collect_identifiers(child, used);
            }
            return;
        }
        SyntaxKind::Ident => {
            if let Some(ident) = node.cast::<ast::Ident>() {
                if ident.as_str() == COL {
                    used.col = true;
                    used.any_column = true;
                } else {
                    used.names.push(ident.as_str().to_owned());
                }
            }
        }
        _ => {}
    }
    for child in node.children() {
        collect_identifiers(child, used);
    }
}

/// The function `col` of the preamble (#501).
const COL: &str = "col";

/// What `text` uses as code. A name can only be used if the text mentions it:
/// the parse is skipped otherwise.
fn used_in(text: &str) -> Used {
    let mut used = Used::default();
    if text.contains(COL)
        || VARIABLES
            .iter()
            .any(|(long, short, _)| text.contains(long) || text.contains(short))
    {
        collect_identifiers(&parse(text), &mut used);
        used.names.retain(|ident| {
            VARIABLES
                .iter()
                .any(|(long, short, _)| ident == long || ident == short)
        });
    }
    used
}

/// One column as a Typst dictionary: both names of every variable.
fn column_dictionary(values: &VariableValues) -> String {
    let mut out = String::from("(");
    for (long, short, field) in VARIABLES {
        let quoted = typst_string(values.get(field));
        out.push_str(&format!("{long}: {quoted}, {short}: {quoted}, "));
    }
    out.push(')');
    out
}

/// `#let col(n) = (…).at(n, default: (…));` over the columns the text names
/// (all of the sheet if it names none by a literal).
fn col_definition(used: &Used, values: &VariableValues) -> String {
    let mut seen: Vec<&str> = Vec::new();
    let mut entries = String::new();
    for column in &values.sheet {
        let name = column.short_name.as_str();
        let wanted = used.any_column || used.literals.iter().any(|literal| literal == name);
        if wanted && !seen.contains(&name) {
            seen.push(name);
            entries.push_str(&format!(
                "{}: {}, ",
                typst_string(name),
                column_dictionary(column)
            ));
        }
    }
    let dictionary = if entries.is_empty() {
        "(:)".to_owned()
    } else {
        format!("({entries})")
    };
    let broken = column_dictionary(&VariableValues::all(&values.unknown));
    format!("#let {COL}(n) = {dictionary}.at(n, default: {broken});")
}

/// `text` with a preamble that defines the variables it uses as code. A text
/// that uses none is returned unchanged. A `#let` of the user in the text
/// comes after the preamble and wins.
pub fn bind(text: &str, values: &VariableValues) -> String {
    let used = used_in(text);
    let mut preamble = String::new();
    for (long, short, field) in VARIABLES {
        let quoted = typst_string(values.get(field));
        for name in [long, short] {
            if used.names.iter().any(|ident| ident == name) {
                preamble.push_str(&format!("#let {name} = {quoted};"));
            }
        }
    }
    if used.col {
        preamble.push_str(&col_definition(&used, values));
    }
    if preamble.is_empty() {
        return text.to_owned();
    }
    format!("{START}{preamble}{END}{text}")
}

/// True if `text` uses a variable as code (a math identifier is none).
fn mentions(text: &str) -> bool {
    let used = used_in(text);
    !used.names.is_empty() || used.col
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
    let values = |text: &str, target: TextTarget| {
        let context = text_context(tree, spec, target);
        match context.and_then(|id| column_fields(tree, id).map(|fields| (id, fields))) {
            Some((id, fields)) => {
                let fields = stokesforge_core::resolve::with_sheet(fields, tree, id, text);
                VariableValues {
                    unknown: texts.broken_reference.clone(),
                    ..VariableValues::from(&fields)
                }
            }
            None => VariableValues::all(&texts.broken_reference),
        }
    };
    let bound = |text: &str, target: TextTarget| -> Option<String> {
        mentions(text).then(|| bind(text, &values(text, target)))
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
        if let Some(after) = rest.strip_prefix(END) {
            return after;
        }
        // `#let …;` — a string may hold anything, `;` and the end marker
        // included: walk to the `;` outside any string, with its escapes.
        let Some(body) = rest.strip_prefix("#let ") else {
            return text;
        };
        let mut chars = body.char_indices();
        let mut in_string = false;
        let mut close = None;
        while let Some((index, character)) = chars.next() {
            match character {
                '\\' if in_string => {
                    chars.next();
                }
                '"' => in_string = !in_string,
                ';' if !in_string => {
                    close = Some(index + 1);
                    break;
                }
                _ => {}
            }
        }
        match close {
            Some(close) => rest = &body[close..],
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

    /// A line marker typed at the start is no list item, bound or not: the
    /// layout puts its own strut on the same line as the text. Witness: a
    /// newline after the preamble turns `- Abs` into a list.
    #[test]
    fn a_line_marker_behaves_like_unbound() {
        let values = values("Abs", "");
        for (typed, plain) in [("- #ln", "- Abs"), ("= #ln", "= Abs"), ("+ #ln", "+ Abs")] {
            let bound = bind(typed, &values);
            assert!(!bound.contains('\n'), "{bound:?}");
            assert_eq!(plain_text(&bound), plain_text(plain), "{typed}");
            assert_eq!(strip_preamble(&bound), typed);
        }
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
        let mut bar = ColorBarSpec {
            title: "#sn".to_owned(),
            ..ColorBarSpec::default()
        };
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

    /// A sheet of two columns, `B` (Absorbance, AU) and `C`.
    fn sheet_values(c_long: &str) -> VariableValues {
        let column = |short: &str, long: &str, units: &str| VariableValues {
            long_name: long.to_owned(),
            short_name: short.to_owned(),
            units: units.to_owned(),
            ..Default::default()
        };
        VariableValues {
            sheet: vec![column("B", "Absorbance", "AU"), column("C", c_long, "cps")],
            unknown: "(broken)".to_owned(),
            ..values("Absorbance", "AU")
        }
    }

    /// #501 lot 4, test 1: a column's field is drawn literally. Witness: put
    /// the value in as markup (no `typst_string`) and `*b*` turns bold.
    #[test]
    fn a_named_column_value_is_drawn_literally() {
        let long = "A *b* \" \\";
        let bound = bind("#col(\"C\").ln", &sheet_values(long));
        assert_eq!(plain_text(&bound).as_deref(), Some(long));
        let both = bind(
            "#col(\"B\").ln/#col(\"C\").u/#col(\"C\").long-name",
            &sheet_values("F"),
        );
        assert_eq!(plain_text(&both).as_deref(), Some("Absorbance/cps/F"));
        assert_eq!(strip_preamble(&bound), "#col(\"C\").ln");
    }

    /// Test 3: an unknown name, or a call with no sheet, reads the broken
    /// reference in every field, and the text still compiles. Witness: no
    /// `default:` in the `.at`, the text fails to compile.
    #[test]
    fn an_unknown_column_reads_the_broken_reference() {
        let bound = bind("#col(\"Z\").ln|#col(\"Z\").xp", &sheet_values("F"));
        assert_eq!(plain_text(&bound).as_deref(), Some("(broken)|(broken)"));
        let alone = VariableValues {
            unknown: "(broken)".to_owned(),
            ..Default::default()
        };
        let bound = bind("#col(\"B\").u", &alone);
        assert_eq!(plain_text(&bound).as_deref(), Some("(broken)"));
    }

    /// A non-literal argument reads the whole sheet.
    #[test]
    fn a_computed_column_name_reads_the_whole_sheet() {
        let bound = bind("#let n = \"C\";#col(n).ln", &sheet_values("F"));
        assert_eq!(plain_text(&bound).as_deref(), Some("F"));
    }

    /// Test 4: `$col$` in math and a text without `col` are byte-identical.
    /// Witness: an unconditional `col` preamble.
    #[test]
    fn col_is_a_variable_only_as_code() {
        for text in ["$col$", "$col(x)$", "color red", "a.col", "Collected"] {
            assert_eq!(bind(text, &sheet_values("F")), text);
            assert!(!mentions(text), "{text}");
        }
        assert!(mentions("#col(\"B\").ln"));
    }

    #[test]
    fn the_users_own_col_wins() {
        let bound = bind(
            "#let col(n) = (ln: \"mine\");#col(\"B\").ln",
            &sheet_values("F"),
        );
        assert_eq!(plain_text(&bound).as_deref(), Some("mine"));
    }

    /// Test 6: the field diagnosis accepts the five variables of any column
    /// and refuses another field. Witness: a plain `diagnose` (no preamble)
    /// refuses `col` itself.
    #[test]
    fn the_diagnosis_knows_col_and_its_fields() {
        assert_eq!(diagnose_text("#col(\"C\").ln"), None);
        assert_eq!(diagnose_text("#col(\"Z\").u"), None);
        assert_eq!(diagnose_text("#col(\"C\").xp #col(\"C\").comments"), None);
        assert!(diagnose_text("#col(\"C\").nope").is_some());
    }
}

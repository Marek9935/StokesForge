//! Text format: the Typst tags that the format bar inserts (issue #63).
//!
//! Titles, axis titles, legend entries and free texts are already **Typst
//! markup** (`docs/rendu.md`, issue #32): making a word bold means wrapping it
//! in `#strong[…]`. So no field is added to the model or to the `.stf` for
//! bold, italic, color and size of a passage; the bar writes into the text,
//! and each write goes through the same undoable action as typing. **The font
//! is no longer part of it** (issue #78): it is a field of the zone, set by
//! `StokesForgeApp::format_zone`; [`TextFormat::Font`] stays what the bar
//! asks for, but reaches this module only when no zone is targeted.
//!
//! Everything here is pure (text and character indices), tested without a
//! window. The indices are **character indices**, like those of egui's cursor.

use std::ops::Range;

/// A format that the bar knows how to apply.
#[derive(Debug, Clone, PartialEq)]
pub enum TextFormat {
    Bold,
    Italic,
    Subscript,
    Superscript,
    /// Math mode `$…$`.
    Formula,
    /// Color `#RRGGBB` or `#RRGGBBAA`. An empty (automatic) or invalid color
    /// applies nothing.
    Color(String),
    /// Size in Typst points.
    Size(f64),
    /// Removes the zone's explicit size (inherited from the rendering).
    AutoSize,
    /// Family name of a font, embedded or from the system.
    Font(String),
    /// Alignment of the text in its box (#244): a field of the zone, never a
    /// tag.
    Align(stokesforge_plot::TextAlign),
}

/// The text after formatting, and the selection to hand back to the field.
#[derive(Debug, Clone, PartialEq)]
pub struct Formatted {
    pub text: String,
    /// Character indices. Empty (`n..n`): cursor only.
    pub selection: Range<usize>,
}

impl TextFormat {
    /// The opening and closing tags, or `None` if the setting applies nothing.
    fn tags(&self) -> Option<(String, String)> {
        let call = |head: String| Some((format!("#{head}["), "]".to_owned()));
        match self {
            TextFormat::Bold => call("strong".into()),
            TextFormat::Italic => call("emph".into()),
            TextFormat::Subscript => call("sub".into()),
            TextFormat::Superscript => call("super".into()),
            TextFormat::Formula => Some(("$".into(), "$".into())),
            TextFormat::Color(color) if valid_color(color) => {
                call(format!("text(fill: rgb(\"{color}\"))"))
            }
            TextFormat::Size(size) if size.is_finite() && *size > 0.0 => {
                call(format!("text(size: {size}pt)"))
            }
            TextFormat::Color(_)
            | TextFormat::Size(_)
            | TextFormat::AutoSize
            | TextFormat::Align(_) => None,
            TextFormat::Font(name) => {
                let name = name.trim();
                if name.is_empty() {
                    None
                } else {
                    let quoted = name.replace('\\', "\\\\").replace('"', "\\\"");
                    call(format!("text(font: \"{quoted}\")"))
                }
            }
        }
    }

    /// These settings are undone by applying them again (bold, italic,
    /// subscript, superscript, formula); color, size and font stack.
    fn toggles(&self) -> bool {
        matches!(
            self,
            TextFormat::Bold
                | TextFormat::Italic
                | TextFormat::Subscript
                | TextFormat::Superscript
                | TextFormat::Formula
        )
    }
}

fn valid_color(color: &str) -> bool {
    let hex = color.strip_prefix('#').unwrap_or("");
    matches!(hex.len(), 6 | 8) && hex.chars().all(|c| c.is_ascii_hexdigit())
}

fn byte_at(text: &str, chars: usize) -> usize {
    text.char_indices()
        .nth(chars)
        .map_or(text.len(), |(byte, _)| byte)
}

/// Applies `format` to `selection` of `text`.
///
/// - non-empty selection: it is wrapped, and stays selected inside the
///   tags;
/// - cursor only: the tags are inserted and the cursor is placed between
///   them;
/// - a toggle setting already applied exactly to the selection (tags inside
///   the selection or just around it) is removed.
///
/// Indices outside the text are clamped into it, a reversed selection is put
/// back in order. A setting that applies nothing returns the text unchanged.
pub fn apply(text: &str, selection: Range<usize>, format: &TextFormat) -> Formatted {
    let len = text.chars().count();
    let (start, end) = if selection.start <= selection.end {
        (selection.start, selection.end)
    } else {
        (selection.end, selection.start)
    };
    let (start, end) = (start.min(len), end.min(len));
    let unchanged = Formatted {
        text: text.to_owned(),
        selection: start..end,
    };
    let Some((open, close)) = format.tags() else {
        return unchanged;
    };
    let (open_len, close_len) = (open.chars().count(), close.chars().count());
    let (from, to) = (byte_at(text, start), byte_at(text, end));
    let selected = &text[from..to];

    if format.toggles() && start != end {
        // The tags are inside the selection.
        if selected.len() >= open.len() + close.len()
            && selected.starts_with(&open)
            && selected.ends_with(&close)
        {
            let inner = &selected[open.len()..selected.len() - close.len()];
            return Formatted {
                text: format!("{}{inner}{}", &text[..from], &text[to..]),
                selection: start..end - open_len - close_len,
            };
        }
        // The tags are just around the selection.
        if text[..from].ends_with(&open) && text[to..].starts_with(&close) {
            return Formatted {
                text: format!(
                    "{}{selected}{}",
                    &text[..from - open.len()],
                    &text[to + close.len()..]
                ),
                selection: start - open_len..end - open_len,
            };
        }
    }

    Formatted {
        text: format!("{}{open}{selected}{close}{}", &text[..from], &text[to..]),
        selection: start + open_len..end + open_len,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn applied(text: &str, selection: Range<usize>, format: TextFormat) -> (String, Range<usize>) {
        let done = apply(text, selection, &format);
        (done.text, done.selection)
    }

    #[test]
    fn a_selection_is_wrapped_and_stays_selected_inside() {
        let (text, selection) = applied("Abs 280 nm", 4..7, TextFormat::Bold);
        assert_eq!(text, "Abs #strong[280] nm");
        assert_eq!(selection, 12..15);
        let chars: Vec<char> = text.chars().collect();
        assert_eq!(chars[selection].iter().collect::<String>(), "280");
    }

    #[test]
    fn a_lone_cursor_inserts_the_tags_and_sits_inside() {
        let (text, selection) = applied("ab", 1..1, TextFormat::Subscript);
        assert_eq!(text, "a#sub[]b");
        assert_eq!(selection, 6..6);
        assert_eq!(
            applied("", 0..0, TextFormat::Formula),
            ("$$".to_owned(), 1..1)
        );
    }

    #[test]
    fn bold_italic_subscript_superscript_formula_each_have_their_tag() {
        for (format, expected) in [
            (TextFormat::Bold, "#strong[x]"),
            (TextFormat::Italic, "#emph[x]"),
            (TextFormat::Subscript, "#sub[x]"),
            (TextFormat::Superscript, "#super[x]"),
            (TextFormat::Formula, "$x$"),
        ] {
            assert_eq!(applied("x", 0..1, format).0, expected);
        }
    }

    #[test]
    fn color_size_and_font_apply_a_text_call() {
        assert_eq!(
            applied("x", 0..1, TextFormat::Color("#ff0000".into())).0,
            "#text(fill: rgb(\"#ff0000\"))[x]"
        );
        assert_eq!(
            applied("x", 0..1, TextFormat::Size(14.0)).0,
            "#text(size: 14pt)[x]"
        );
        assert_eq!(
            applied("x", 0..1, TextFormat::Font("Fira Sans".into())).0,
            "#text(font: \"Fira Sans\")[x]"
        );
    }

    #[test]
    fn a_font_name_with_a_quote_is_escaped() {
        assert_eq!(
            applied("x", 0..1, TextFormat::Font("A\"B".into())).0,
            "#text(font: \"A\\\"B\")[x]"
        );
    }

    #[test]
    fn a_setting_that_applies_nothing_leaves_the_text() {
        for format in [
            TextFormat::Color(String::new()),
            TextFormat::Color("rouge".into()),
            TextFormat::Size(f64::NAN),
            TextFormat::Size(0.0),
            TextFormat::Font("  ".into()),
        ] {
            assert_eq!(applied("x", 0..1, format), ("x".to_owned(), 0..1));
        }
    }

    #[test]
    fn applying_a_toggle_setting_again_removes_it() {
        // Tags around the selection.
        let (text, selection) = applied("a #strong[280] b", 10..13, TextFormat::Bold);
        assert_eq!(text, "a 280 b");
        assert_eq!(selection, 2..5);
        // Tags inside the selection.
        let (text, selection) = applied("a #emph[280] b", 2..12, TextFormat::Italic);
        assert_eq!(text, "a 280 b");
        assert_eq!(selection, 2..5);
        // Another setting undoes nothing.
        assert_eq!(
            applied("#strong[x]", 8..9, TextFormat::Italic).0,
            "#strong[#emph[x]]"
        );
        // Color and size stack.
        assert_eq!(
            applied("x", 0..1, TextFormat::Size(9.0)).0,
            "#text(size: 9pt)[x]"
        );
    }

    #[test]
    fn indices_are_characters_not_bytes() {
        let (text, selection) = applied("λ nm é", 0..1, TextFormat::Italic);
        assert_eq!(text, "#emph[λ] nm é");
        assert_eq!(selection, 6..7);
        let (text, _) = applied("λ nm é", 5..6, TextFormat::Bold);
        assert_eq!(text, "λ nm #strong[é]");
    }

    #[test]
    #[allow(clippy::reversed_empty_ranges)]
    fn a_reversed_or_out_of_range_selection_is_clamped_inside() {
        assert_eq!(applied("abc", 3..1, TextFormat::Bold).0, "a#strong[bc]");
        assert_eq!(applied("abc", 1..99, TextFormat::Bold).0, "a#strong[bc]");
        assert_eq!(applied("abc", 50..60, TextFormat::Bold).0, "abc#strong[]");
    }
}

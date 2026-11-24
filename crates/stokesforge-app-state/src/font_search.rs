//! The pure filter of the font list (#554): which rows the list shows for a
//! search query, and how the keyboard moves over them. No drawing here; the
//! interface only paints the rows this module returns.
//!
//! Rows come flat, in the order Default, embedded title and its families,
//! system title and its families. Names are borrowed, never cloned.

use stokesforge_render::FamilyEntry;

/// The two groups of the list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Section {
    /// Fonts shipped inside StokesForge.
    Embedded,
    /// Fonts installed on this PC (and the current one if it is missing).
    System,
}

/// One row of the filtered list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Row<'a> {
    /// The « Graph font » row (empty family); carries its translated label.
    Default(&'a str),
    /// A group title; never selectable.
    Title(Section),
    /// A family name.
    Family(&'a str),
}

impl Row<'_> {
    /// Whether a click or Enter can pick this row (everything but a title).
    pub fn is_selectable(&self) -> bool {
        !matches!(self, Row::Title(_))
    }
}

/// Which way [`step`] moves.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Up,
    Down,
}

/// Whether every whitespace-separated word of the query (already lowercased)
/// is somewhere in `name`, in any order, ignoring case but not accents. No
/// word (blank query) matches everything.
fn matches(name: &str, words: &[String]) -> bool {
    if words.is_empty() {
        return true;
    }
    let lower = name.to_lowercase();
    words.iter().all(|word| lower.contains(word.as_str()))
}

/// The rows to show for `query`. `current` is the family of the setting: if
/// it is non-empty and not in `families` (font missing on this PC) it is
/// listed right after the Default row, before the titles and with no title of
/// its own (it is neither included nor installed), filtered like the others.
/// `default_label` is the translated « Graph font », filtered like a name. A
/// title with no matching family is left out.
pub fn rows<'a>(
    families: &'a [FamilyEntry],
    current: &'a str,
    default_label: &'a str,
    query: &str,
) -> Vec<Row<'a>> {
    let words: Vec<String> = query.split_whitespace().map(str::to_lowercase).collect();
    let mut out = Vec::new();
    if matches(default_label, &words) {
        out.push(Row::Default(default_label));
    }
    let missing =
        (!current.is_empty() && !families.iter().any(|f| f.name == current)).then_some(current);
    out.extend(
        missing
            .filter(|name| matches(name, &words))
            .map(Row::Family),
    );
    for (section, embedded) in [(Section::Embedded, true), (Section::System, false)] {
        let title_at = out.len();
        out.push(Row::Title(section));
        out.extend(
            families
                .iter()
                .filter(|f| f.embedded == embedded && matches(&f.name, &words))
                .map(|f| Row::Family(f.name.as_str())),
        );
        if out.len() == title_at + 1 {
            out.pop();
        }
    }
    out
}

/// The row Enter picks: the first one that is not a title.
pub fn first_selectable(rows: &[Row<'_>]) -> Option<usize> {
    rows.iter().position(Row::is_selectable)
}

/// The next selectable row after `from` in `direction`, skipping titles. At
/// either end it stays on `from` (no wrap).
pub fn step(rows: &[Row<'_>], from: usize, direction: Direction) -> usize {
    let found = match direction {
        Direction::Down => rows
            .iter()
            .enumerate()
            .skip(from + 1)
            .find(|(_, row)| row.is_selectable())
            .map(|(index, _)| index),
        Direction::Up => rows
            .iter()
            .enumerate()
            .take(from)
            .rev()
            .find(|(_, row)| row.is_selectable())
            .map(|(index, _)| index),
    };
    found.unwrap_or(from)
}

/// The row of the current font: Default for an empty family, else the family
/// of that name; `None` when the filter hides it.
pub fn current_index(rows: &[Row<'_>], current: &str) -> Option<usize> {
    rows.iter().position(|row| match row {
        Row::Default(_) => current.is_empty(),
        Row::Family(name) => *name == current,
        Row::Title(_) => false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fam(name: &str, embedded: bool) -> FamilyEntry {
        FamilyEntry {
            name: name.to_owned(),
            embedded,
        }
    }

    fn sample() -> Vec<FamilyEntry> {
        vec![
            fam("Noto Sans", true),
            fam("Noto Serif", true),
            fam("Fira Code", false),
            fam("Noto Sans Mono", false),
            fam("Ubuntu", false),
        ]
    }

    fn names<'a>(rows: &[Row<'a>]) -> Vec<&'a str> {
        rows.iter()
            .filter_map(|row| match row {
                Row::Family(name) => Some(*name),
                _ => None,
            })
            .collect()
    }

    fn titles(rows: &[Row<'_>]) -> usize {
        rows.iter().filter(|r| matches!(r, Row::Title(_))).count()
    }

    #[test]
    fn a_word_is_found_anywhere_in_the_name() {
        let f = sample();
        let r = rows(&f, "", "Graph font", "ono");
        assert_eq!(names(&r), vec!["Noto Sans Mono"]);
        let r = rows(&f, "", "Graph font", "noto");
        assert_eq!(names(&r), vec!["Noto Sans", "Noto Serif", "Noto Sans Mono"]);
        let r = rows(&f, "", "Graph font", "sans");
        assert_eq!(names(&r), vec!["Noto Sans", "Noto Sans Mono"]);
    }

    #[test]
    fn case_is_ignored() {
        let f = sample();
        assert_eq!(
            rows(&f, "", "Graph font", "FIRA"),
            rows(&f, "", "Graph font", "fira")
        );
        assert_eq!(
            names(&rows(&f, "", "Graph font", "FIRA")),
            vec!["Fira Code"]
        );
    }

    #[test]
    fn words_match_in_any_order() {
        let f = sample();
        let r = rows(&f, "", "Graph font", "mono noto");
        assert_eq!(names(&r), vec!["Noto Sans Mono"]);
    }

    #[test]
    fn a_blank_query_gives_every_row() {
        let f = sample();
        for query in ["", "   "] {
            let r = rows(&f, "", "Graph font", query);
            assert_eq!(r[0], Row::Default("Graph font"));
            assert_eq!(titles(&r), 2);
            assert_eq!(names(&r).len(), 5);
            assert_eq!(r[1], Row::Title(Section::Embedded));
        }
    }

    #[test]
    fn no_match_gives_no_row_and_no_title() {
        let f = sample();
        assert!(rows(&f, "", "Graph font", "zzz").is_empty());
    }

    #[test]
    fn a_title_without_family_is_left_out() {
        let f = sample();
        let r = rows(&f, "", "Graph font", "fira");
        assert_eq!(
            r,
            vec![Row::Title(Section::System), Row::Family("Fira Code")]
        );
    }

    #[test]
    fn the_default_row_is_filtered_like_a_name() {
        let f = sample();
        assert!(!rows(&f, "", "Graph font", "ubu").contains(&Row::Default("Graph font")));
        assert!(rows(&f, "", "Graph font", "graph").contains(&Row::Default("Graph font")));
    }

    /// A current font missing on this PC is listed right after the Default
    /// row, under no title. Witness: pushing it back into the system group
    /// puts it last, under « Installed on this computer ».
    #[test]
    fn a_font_not_installed_here_is_listed_after_the_default_row_without_a_title() {
        let f = sample();
        let r = rows(&f, "Calibri", "Graph font", "");
        assert_eq!(r[0], Row::Default("Graph font"));
        assert_eq!(r[1], Row::Family("Calibri"));
        assert_eq!(r[2], Row::Title(Section::Embedded));
        assert_eq!(titles(&r), 2);
        assert!(rows(&f, "Calibri", "Graph font", "zzz").is_empty());
        let r = rows(&f, "Calibri", "Graph font", "cali");
        assert_eq!(r, vec![Row::Family("Calibri")]);
        // An installed current family is not listed twice.
        let r = rows(&f, "Ubuntu", "Graph font", "");
        assert_eq!(names(&r).iter().filter(|n| **n == "Ubuntu").count(), 1);
    }

    #[test]
    fn enter_skips_a_title() {
        let f = sample();
        let r = rows(&f, "", "Graph font", "fira");
        assert_eq!(first_selectable(&r), Some(1));
        let r = rows(&f, "", "Graph font", "");
        assert_eq!(first_selectable(&r), Some(0));
        assert_eq!(first_selectable(&[]), None);
    }

    #[test]
    fn stepping_skips_titles_and_does_not_wrap() {
        let f = sample();
        let r = rows(&f, "", "Graph font", "");
        // Default, T, Noto Sans, Noto Serif, T, Fira, Noto Mono, Ubuntu
        assert_eq!(step(&r, 0, Direction::Down), 2);
        assert_eq!(step(&r, 3, Direction::Down), 5);
        assert_eq!(step(&r, 5, Direction::Up), 3);
        assert_eq!(step(&r, 2, Direction::Up), 0);
        assert_eq!(step(&r, 0, Direction::Up), 0);
        let last = r.len() - 1;
        assert_eq!(step(&r, last, Direction::Down), last);
    }

    #[test]
    fn the_current_font_has_its_row() {
        let f = sample();
        let r = rows(&f, "Fira Code", "Graph font", "");
        assert_eq!(current_index(&r, "Fira Code"), Some(5));
        assert_eq!(current_index(&r, ""), Some(0));
        let r = rows(&f, "Fira Code", "Graph font", "ubu");
        assert_eq!(current_index(&r, "Fira Code"), None);
    }
}

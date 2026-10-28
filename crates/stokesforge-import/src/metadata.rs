//! Metadata of an imported file: the header (and, if the recipe asks for it,
//! the footer) split into key / value lines, in file order. Vocabulary
//! specific to this crate (it does not know `stokesforge-core`):
//! `stokesforge-app` converts it to `stokesforge_core::MetaLine`.

use crate::parse::EffectiveSeparator;
use crate::recipe::ImportRecipe;

/// A metadata line. Three forms: a section title
/// (`key = "[Measurement Information]"`, empty `value`), a key / value pair
/// (`XUNITS;NANOMETERS`), or a line that has neither form, kept as is (empty
/// `key`, the whole line in `value`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MetadataLine {
    pub key: String,
    pub value: String,
}

impl MetadataLine {
    /// True for a section title.
    pub fn is_section(&self) -> bool {
        self.value.is_empty() && self.key.starts_with('[') && self.key.ends_with(']')
    }
}

/// Splits `lines` into [`MetadataLine`]s with the file's effective separator.
/// Empty lines are ignored; `recipe.excluded_metadata` drops keys, or whole
/// sections (up to the next section title).
pub(crate) fn split_lines<'a>(
    lines: impl Iterator<Item = &'a str>,
    separator: &EffectiveSeparator,
    recipe: &ImportRecipe,
) -> Vec<MetadataLine> {
    let excluded: Vec<String> = recipe
        .excluded_metadata
        .iter()
        .map(|name| name.trim().to_lowercase())
        .collect();
    let is_excluded = |key: &str| excluded.contains(&key.trim().to_lowercase());
    let mut out = Vec::new();
    let mut in_excluded_section = false;
    for line in lines {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        // A section title can trail separators (`[Section];;`).
        let bare = trimmed.trim_end_matches([';', ',', '\t', ' ']);
        let entry = if bare.starts_with('[') && bare.ends_with(']') {
            MetadataLine {
                key: bare.to_string(),
                value: String::new(),
            }
        } else if let Some((key, value)) = separator.split_key_value(trimmed) {
            if key.trim().is_empty() {
                MetadataLine {
                    key: String::new(),
                    value: trimmed.to_string(),
                }
            } else {
                MetadataLine {
                    key: key.trim().to_string(),
                    value: value.trim().to_string(),
                }
            }
        } else {
            MetadataLine {
                key: String::new(),
                value: trimmed.to_string(),
            }
        };
        if entry.is_section() {
            in_excluded_section = is_excluded(&entry.key)
                || is_excluded(entry.key.trim_start_matches('[').trim_end_matches(']'));
            if in_excluded_section {
                continue;
            }
        } else if in_excluded_section || is_excluded(&entry.key) {
            continue;
        }
        out.push(entry);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn split(lines: &[&str], recipe: &ImportRecipe) -> Vec<(String, String)> {
        split_lines(
            lines.iter().copied(),
            &EffectiveSeparator::Char(';'),
            recipe,
        )
        .into_iter()
        .map(|l| (l.key, l.value))
        .collect()
    }

    #[test]
    fn keys_sections_and_raw_lines() {
        let recipe = ImportRecipe::new("t");
        let got = split(
            &[
                "TITLE;S13",
                "[Measurement Information];;",
                "",
                "XUNITS;NANOMETERS",
                "##### Extended",
            ],
            &recipe,
        );
        assert_eq!(
            got,
            vec![
                ("TITLE".into(), "S13".into()),
                ("[Measurement Information]".into(), String::new()),
                ("XUNITS".into(), "NANOMETERS".into()),
                (String::new(), "##### Extended".into()),
            ]
        );
    }

    #[test]
    fn exclusion_by_key_and_by_section() {
        let mut recipe = ImportRecipe::new("t");
        recipe.excluded_metadata = vec!["date".into(), "[Extended]".into()];
        let got = split(
            &[
                "DATE;25/11/24",
                "TITLE;S13",
                "[Extended]",
                "A;1",
                "B;2",
                "[Other]",
                "C;3",
            ],
            &recipe,
        );
        let keys: Vec<&str> = got.iter().map(|(k, _)| k.as_str()).collect();
        assert_eq!(keys, vec!["TITLE", "[Other]", "C"]);
    }
}

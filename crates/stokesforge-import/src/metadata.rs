//! Métadonnées d'un fichier importé : l'en-tête (et, si la recette le demande,
//! le pied de page) découpé en lignes clé / valeur, dans l'ordre du fichier.
//! Vocabulaire propre à ce crate (il ne connaît pas `stokesforge-core`) : `stokesforge-app`
//! le convertit en `stokesforge_core::MetaLine`.

use crate::parse::EffectiveSeparator;
use crate::recipe::ImportRecipe;

/// Une ligne de métadonnées. Trois formes : un titre de section
/// (`key = "[Measurement Information]"`, `value` vide), un couple clé / valeur
/// (`XUNITS;NANOMETERS`), ou une ligne qui n'a pas cette forme, gardée telle
/// quelle (`key` vide, toute la ligne dans `value`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MetadataLine {
    pub key: String,
    pub value: String,
}

impl MetadataLine {
    /// Vrai pour un titre de section.
    pub fn is_section(&self) -> bool {
        self.value.is_empty() && self.key.starts_with('[') && self.key.ends_with(']')
    }
}

/// Découpe `lines` en [`MetadataLine`] avec le séparateur effectif du fichier.
/// Les lignes vides sont ignorées ; `recipe.excluded_metadata` écarte des
/// clés, ou des sections entières (jusqu'au titre de section suivant).
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
        // Un titre de section peut traîner des séparateurs (`[Section];;`).
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

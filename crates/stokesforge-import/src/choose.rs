//! Automatic choice of the template from a file: extension, part of the name,
//! signature in the start of the content (`ORIGIN;JASCO`...). The principle of
//! Origin's "Import Filters".

use std::path::Path;

use crate::recipe::ImportRecipe;

/// How many bytes from the start of the file are searched for the content
/// signature.
const SIGNATURE_BYTES: usize = 4096;

/// The template in `recipes` that best matches the file `path` (whose content
/// is `bytes`), or `None` if none matches.
///
/// A template matches if **all** its non-empty criteria
/// ([`crate::recipe::AutoMatch`]) are met; a template without criteria is
/// never chosen. The most specific wins: the content signature weighs 4, the
/// name 2, the extension 1; on a tie, the first in the list.
pub fn choose_recipe<'a>(
    recipes: &'a [ImportRecipe],
    path: &Path,
    bytes: &[u8],
) -> Option<&'a ImportRecipe> {
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default()
        .to_lowercase();
    let extension = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or_default()
        .to_lowercase();
    let head = &bytes[..bytes.len().min(SIGNATURE_BYTES)];
    let head = crate::decode::decode_text(head)
        .map(|text| text.to_lowercase())
        .unwrap_or_default();

    let mut best: Option<(&ImportRecipe, u32)> = None;
    for recipe in recipes {
        let criteria = &recipe.auto_match;
        if criteria.is_empty() {
            continue;
        }
        let mut score = 0;
        if !criteria.extensions.is_empty() {
            let ok = criteria.extensions.iter().any(|candidate| {
                candidate.trim().trim_start_matches('.').to_lowercase() == extension
            });
            if !ok {
                continue;
            }
            score += 1;
        }
        let wanted_name = criteria.name_contains.trim().to_lowercase();
        if !wanted_name.is_empty() {
            if !name.contains(&wanted_name) {
                continue;
            }
            score += 2;
        }
        let signature = criteria.content_contains.trim().to_lowercase();
        if !signature.is_empty() {
            if !head.contains(&signature) {
                continue;
            }
            score += 4;
        }
        if best.is_none_or(|(_, top)| score > top) {
            best = Some((recipe, score));
        }
    }
    best.map(|(recipe, _)| recipe)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::recipe::AutoMatch;

    fn recipe(name: &str, extensions: &[&str], name_contains: &str, content: &str) -> ImportRecipe {
        let mut recipe = ImportRecipe::new(name);
        recipe.auto_match = AutoMatch {
            extensions: extensions.iter().map(|e| e.to_string()).collect(),
            name_contains: name_contains.into(),
            content_contains: content.into(),
        };
        recipe
    }

    #[test]
    fn content_signature_beats_extension() {
        let recipes = vec![
            recipe("CSV", &["csv"], "", ""),
            recipe("Jasco", &["csv"], "", "ORIGIN;JASCO"),
        ];
        let chosen = choose_recipe(
            &recipes,
            Path::new("/x/S13.CSV"),
            b"TITLE;a
ORIGIN;JASCO
",
        );
        assert_eq!(chosen.map(|r| r.name.as_str()), Some("Jasco"));
        let other = choose_recipe(
            &recipes,
            Path::new("/x/a.csv"),
            b"1;2
",
        );
        assert_eq!(other.map(|r| r.name.as_str()), Some("CSV"));
    }

    #[test]
    fn all_non_empty_criteria_must_match() {
        let recipes = vec![recipe("Fluo", &["txt"], "fluo", "origin\tjasco")];
        assert!(choose_recipe(&recipes, Path::new("a_fluo.txt"), b"ORIGIN\tJASCO").is_some());
        assert!(choose_recipe(&recipes, Path::new("a.txt"), b"ORIGIN\tJASCO").is_none());
        assert!(choose_recipe(&recipes, Path::new("a_fluo.csv"), b"ORIGIN\tJASCO").is_none());
        assert!(choose_recipe(&recipes, Path::new("a_fluo.txt"), b"other").is_none());
    }

    #[test]
    fn a_recipe_without_criteria_is_never_chosen() {
        let recipes = vec![ImportRecipe::new("Manual")];
        assert!(choose_recipe(&recipes, Path::new("a.csv"), b"1,2").is_none());
        assert!(choose_recipe(&[], Path::new("a.csv"), b"1,2").is_none());
    }
}

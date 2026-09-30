//! Le découpage d'un fichier en zones — en-tête, sous-en-tête, ligne de noms,
//! données, pied de page, commentaires — calculé **une seule fois** et partagé
//! par [`crate::parse::parse`] (qui en tire les colonnes) et
//! [`crate::preview::preview`] (qui en montre les rôles). Les deux ne peuvent
//! donc plus diverger (issue #23 : ils réécrivaient chacun la même suite
//! d'étapes, avec des entrées différentes).

use crate::numbers::NumberFormat;
use crate::parse::{resolve_separator, EffectiveSeparator, ImportError};
use crate::preview::LineRole;
use crate::recipe::{HeaderSkip, ImportRecipe};

/// Le découpage d'un texte selon une recette.
pub(crate) struct Layout<'a> {
    /// Toutes les lignes du texte, blanches comprises (`\r` final retiré).
    pub raw: Vec<&'a str>,
    /// Première ligne non blanche, et fin (exclue) après retrait des lignes
    /// blanches de queue : la zone analysée.
    pub start: usize,
    pub end: usize,
    /// Rôle de chaque ligne de `raw` ; `Blank` hors de `start..end`.
    pub roles: Vec<LineRole>,
    /// Indices (dans `raw`) des lignes d'en-tête, principal puis sous-en-tête.
    pub header: Vec<usize>,
    /// Position, dans `header`, de la première ligne de sous-en-tête.
    pub subheader_start: usize,
    /// Indice de la ligne de noms de colonnes, si la recette en demande une.
    pub column_header: Option<usize>,
    /// Indices des lignes de données gardées, dans l'ordre.
    pub data: Vec<usize>,
    pub separator: EffectiveSeparator,
    pub numbers: NumberFormat,
}

/// `line` privée de ses `skip_chars` premiers caractères.
pub(crate) fn skip_str(line: &str, skip_chars: usize) -> &str {
    if skip_chars == 0 {
        return line;
    }
    match line.char_indices().nth(skip_chars) {
        Some((byte_idx, _)) => &line[byte_idx..],
        None => "",
    }
}

fn is_comment(line: &str, prefix: &str) -> bool {
    !prefix.is_empty() && line.trim_start().starts_with(prefix)
}

/// Découpe `text` selon `recipe`. Mêmes échecs que [`crate::parse::parse`] :
/// fichier vide, ou plus aucune ligne de donnée après l'en-tête, le pied de
/// page, la ligne de noms ou les lignes ignorées.
pub(crate) fn analyze<'a>(text: &'a str, recipe: &ImportRecipe) -> Result<Layout<'a>, ImportError> {
    let raw: Vec<&str> = text.split('\n').map(|l| l.trim_end_matches('\r')).collect();
    // Lignes blanches aux deux extrémités seulement : une ligne vide au milieu
    // reste à sa place (elle finira toute en NaN plutôt que de décaler
    // discrètement les suivantes).
    let mut start = 0usize;
    while start < raw.len() && raw[start].trim().is_empty() {
        start += 1;
    }
    let mut end = raw.len();
    while end > start && raw[end - 1].trim().is_empty() {
        end -= 1;
    }
    if start >= end {
        return Err(ImportError::Empty);
    }

    let mut roles = vec![LineRole::Blank; raw.len()];
    let mut header: Vec<usize> = Vec::new();
    let mut idx = start;
    let subheader_start;
    // Deux modes d'en-tête : dès que `header_layout` compte une ligne, il
    // remplace `recipe.header` (voir `docs/chantier-import-previsualisation.md`).
    if recipe.header_layout.main_header_lines > 0 || recipe.header_layout.subheader_lines > 0 {
        let main_n = recipe.header_layout.main_header_lines.min(end - idx);
        for _ in 0..main_n {
            roles[idx] = LineRole::MainHeader;
            header.push(idx);
            idx += 1;
        }
        subheader_start = header.len();
        let sub_n = recipe.header_layout.subheader_lines.min(end - idx);
        for s in 0..sub_n {
            roles[idx] = crate::preview::subheader_line_role(s + 1, &recipe.subheader_roles);
            header.push(idx);
            idx += 1;
        }
    } else {
        match &recipe.header {
            HeaderSkip::None => {}
            HeaderSkip::Lines(n) => {
                for _ in 0..(*n).min(end - idx) {
                    roles[idx] = LineRole::MainHeader;
                    header.push(idx);
                    idx += 1;
                }
            }
            HeaderSkip::UntilMarker(marker) => {
                let marker = marker.trim().to_lowercase();
                let mut found = false;
                while idx < end {
                    roles[idx] = LineRole::MainHeader;
                    header.push(idx);
                    let matched = raw[idx].trim().to_lowercase() == marker;
                    idx += 1;
                    if matched {
                        found = true;
                        break;
                    }
                }
                if !found {
                    return Err(ImportError::NoDataRows);
                }
            }
        }
        subheader_start = header.len();
    }

    let footer_start = end - recipe.footer_skip_lines.min(end - idx);
    for role in roles.iter_mut().take(end).skip(footer_start) {
        *role = LineRole::Footer;
    }
    if idx >= footer_start {
        return Err(ImportError::NoDataRows);
    }

    // Lignes restantes, commentaires exclus.
    let mut remaining: Vec<usize> = Vec::with_capacity(footer_start - idx);
    for i in idx..footer_start {
        if is_comment(skip_str(raw[i], recipe.skip_chars), &recipe.comment_prefix)
            || is_comment(raw[i], &recipe.comment_prefix)
        {
            roles[i] = LineRole::Comment;
        } else {
            remaining.push(i);
        }
    }
    if remaining.is_empty() {
        return Err(ImportError::NoDataRows);
    }

    // Format des nombres et séparateur : résolus **sur les mêmes lignes** pour
    // l'import et l'aperçu — les premières lignes restantes (ligne de noms
    // comprise), préfixe retiré.
    let numbers = NumberFormat::resolve(
        recipe,
        remaining
            .iter()
            .map(|&i| skip_str(raw[i], recipe.skip_chars)),
    );
    let separator = resolve_separator(
        &recipe.separator,
        skip_str(raw[remaining[0]], recipe.skip_chars),
        numbers.decimal,
    );

    let column_header = if recipe.has_column_header_row {
        let i = remaining.remove(0);
        roles[i] = LineRole::ColumnHeaderRow;
        Some(i)
    } else {
        None
    };
    // Plage de lignes : les `skip_data_rows` premières sont ignorées.
    let skipped = recipe.skip_data_rows.min(remaining.len());
    for &i in &remaining[..skipped] {
        roles[i] = LineRole::Skipped;
    }
    let candidates = &remaining[skipped..];
    if candidates.is_empty() {
        return Err(ImportError::NoDataRows);
    }

    // Fin des données : la coupure, puis le pied de page.
    let kept = crate::data_end::data_len(
        candidates
            .iter()
            .map(|&i| skip_str(raw[i], recipe.skip_chars)),
        candidates.len(),
        &recipe.data_end,
        &separator,
        &numbers,
        recipe.column_kind(0),
    );
    if kept == 0 {
        return Err(ImportError::NoDataRows);
    }
    let data: Vec<usize> = candidates[..kept].to_vec();
    for &i in &data {
        roles[i] = LineRole::Data;
    }
    let last_data = data[kept - 1];
    for role in roles.iter_mut().take(end).skip(last_data + 1) {
        if matches!(*role, LineRole::Blank | LineRole::Data) {
            *role = LineRole::Footer;
        }
    }
    // Les commentaires au-delà de la dernière donnée restent des
    // commentaires ; toute autre ligne restante est du pied de page.
    Ok(Layout {
        raw,
        start,
        end,
        roles,
        header,
        subheader_start,
        column_header,
        data,
        separator,
        numbers,
    })
}

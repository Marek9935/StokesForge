//! Le moteur de parsing : applique une [`crate::ImportRecipe`] à des octets
//! bruts pour en tirer un [`ParsedTable`]. Aucun jeton illisible ni aucune
//! ligne mal formée n'interrompt l'import — voir l'invariant 10
//! (« un défaut de donnée dégrade, il ne plante pas ») : les valeurs
//! non-analysables deviennent `f64::NAN`, jamais une erreur.
//!
//! Seuls trois cas font échouer tout l'import : un fichier qui n'est pas du
//! texte (binaire — voir [`crate::decode`] pour les encodages acceptés), un
//! fichier vide, ou un fichier dont on ne trouve aucune ligne de
//! donnée après application de la recette (marqueur d'en-tête introuvable,
//! ou plus rien après en-tête/pied de page).

/// Une colonne telle que lue dans le fichier.
#[derive(Debug, Clone, PartialEq)]
pub struct ParsedColumn {
    /// Nom lu sur la ligne d'en-tête de colonnes, si `has_column_header_row`
    /// et que cette colonne y avait une entrée non vide.
    pub name: Option<String>,
    /// Une valeur par ligne de données, dans l'ordre. `f64::NAN` pour une
    /// valeur manquante ou non analysable — jamais d'erreur bloquante pour
    /// un jeton individuel illisible (invariant 10).
    pub values: Vec<f64>,
    /// Le jeton d'origine (`trim`é) pour chaque ligne où `values` porte un
    /// `NaN` **parce que le jeton n'était pas un nombre** — `None` sinon (le
    /// jeton était absent, vide, ou s'est bien analysé). Même longueur que
    /// `values`. Sert à distinguer, en aval, une vraie valeur manquante d'un
    /// texte à conserver (`plotyx_core::ColumnType::Mixed`, chantier
    /// « colonnes à types mixtes ») — c'est le premier consommateur réel de
    /// ce type : sans ce champ, `1,2 / n/a / 1,5` perdait le `n/a`.
    pub texts: Vec<Option<String>>,
    /// Nom long extrait des lignes de sous-en-tête assignées au rôle
    /// « Long Names » ([`crate::recipe::SubheaderRoles::long_name`]), le cas
    /// échéant.
    pub long_name: Option<String>,
    /// Unité extraite des lignes de sous-en-tête assignées au rôle
    /// « Units ».
    pub unit: Option<String>,
    /// Commentaire extrait des lignes de sous-en-tête assignées au rôle
    /// « Comments ».
    pub comment: Option<String>,
}

/// Le séparateur effectivement utilisé pour ce fichier, une fois
/// `Separator::Auto` (et `Separator::Whitespace`) résolu. `pub(crate)` :
/// utile aussi à [`crate::preview`] (aperçu en direct) et à la détection
/// automatique des lignes d'en-tête, pas seulement à retrouver les couples
/// clé/valeur des lignes d'en-tête ignorées.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum EffectiveSeparator {
    Char(char),
    Whitespace,
}

impl EffectiveSeparator {
    /// Découpe une ligne en jetons selon ce séparateur. Ne retire jamais les
    /// jetons vides pour un séparateur caractère (un CSV `a;;c` a bien 3
    /// champs, le second vide) ; `Whitespace` les absorbe naturellement.
    pub(crate) fn tokenize(self, line: &str) -> Vec<&str> {
        match self {
            EffectiveSeparator::Char(c) => line.split(c).collect(),
            EffectiveSeparator::Whitespace => line.split_whitespace().collect(),
        }
    }

    /// Coupe une ligne « CLÉ<séparateur>VALEUR » en `(clé, valeur)` à la
    /// première occurrence du séparateur.
    fn split_key_value(self, line: &str) -> Option<(&str, &str)> {
        match self {
            EffectiveSeparator::Char(c) => line.split_once(c),
            EffectiveSeparator::Whitespace => line.split_once(char::is_whitespace),
        }
    }
}

/// Le résultat d'un import réussi : les colonnes lues, plus toute anomalie
/// non bloquante constatée en chemin.
#[derive(Debug, Clone, PartialEq)]
pub struct ParsedTable {
    pub columns: Vec<ParsedColumn>,
    /// Anomalies non bloquantes constatées pendant le parsing (ex. un
    /// nombre de points annoncé dans l'en-tête qui ne correspond pas au
    /// nombre de lignes réellement lues) — jamais vide en cas d'erreur, ce
    /// sont des avertissements sur un import qui a réussi.
    pub warnings: Vec<String>,
    header_lines: Vec<String>,
    separator: EffectiveSeparator,
}

impl ParsedTable {
    /// Nombre de lignes de données (0 si `columns` est vide).
    pub fn row_count(&self) -> usize {
        self.columns.first().map_or(0, |c| c.values.len())
    }

    /// Cherche, parmi les lignes d'en-tête ignorées lors du parsing, une
    /// ligne de la forme « CLÉ<séparateur>VALEUR » où CLÉ correspond à `key`
    /// (insensible à la casse, bordures blanches ignorées) — rend VALEUR
    /// (`trim`ée), ou `None` si absente. `<séparateur>` est celui
    /// effectivement résolu pour ce fichier (utile même quand la recette
    /// demandait `Separator::Auto`).
    pub fn header_value(&self, key: &str) -> Option<String> {
        lookup_header_value(&self.header_lines, self.separator, key)
    }
}

/// Cherche `key` dans `header_lines`, sous la forme CLÉ<séparateur>VALEUR.
fn lookup_header_value(
    header_lines: &[String],
    separator: EffectiveSeparator,
    key: &str,
) -> Option<String> {
    let key_lower = key.trim().to_lowercase();
    header_lines.iter().find_map(|line| {
        let (k, v) = separator.split_key_value(line)?;
        if k.trim().to_lowercase() == key_lower {
            Some(v.trim().to_string())
        } else {
            None
        }
    })
}

/// Les façons dont un import peut échouer entièrement — tout le reste
/// dégrade plutôt que d'échouer (invariant 10).
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum ImportError {
    #[error("le fichier n'est pas du texte lisible (fichier binaire)")]
    NotText,
    #[error("le fichier est vide")]
    Empty,
    #[error("aucune ligne de données trouvée après l'en-tête")]
    NoDataRows,
}

use crate::recipe::{DecimalStyle, HeaderSkip, ImportRecipe, NameSource, Separator};

/// Parse `bytes` selon `recipe`. `Err(ImportError::NotText)` si `bytes`
/// n'est pas du texte ([`crate::decode::decode_text`] : UTF-8, UTF-16 avec
/// marque, ou Windows-1252) — jamais de panique sur un fichier binaire.
pub fn parse(bytes: &[u8], recipe: &ImportRecipe) -> Result<ParsedTable, ImportError> {
    if bytes.is_empty() {
        return Err(ImportError::Empty);
    }
    let text = crate::decode::decode_text(bytes)?;

    // Découpe en lignes, tolère le CRLF, ne filtre les lignes vides qu'aux
    // deux extrémités — une ligne de données vide au milieu du fichier
    // reste à sa place (elle finira comme une ligne toute en NaN plutôt que
    // d'être silencieusement absorbée : plus robuste face à un fichier
    // partiellement corrompu que de décaler discrètement toutes les lignes
    // suivantes).
    let mut lines: Vec<&str> = text.split('\n').map(|l| l.trim_end_matches('\r')).collect();
    while lines.first().is_some_and(|l| l.trim().is_empty()) {
        lines.remove(0);
    }
    while lines.last().is_some_and(|l| l.trim().is_empty()) {
        lines.pop();
    }
    if lines.is_empty() {
        return Err(ImportError::Empty);
    }

    // Applique le saut d'en-tête, en mémorisant les lignes ignorées.
    //
    // Deux modes coexistent, voir `docs/chantier-import-previsualisation.md`
    // (« l'en-tête structuré ne remplace le legacy que si l'utilisateur s'en
    // sert ») : dès que `header_layout` compte au moins une ligne (principale
    // ou de sous-en-tête), il **remplace** entièrement `recipe.header` pour
    // cet import — sinon `recipe.header` (`None`/`Lines`/`UntilMarker`)
    // continue de piloter le saut comme avant ce chantier, aucune régression
    // pour une recette qui n'utilise pas le nouveau modèle.
    let mut header_lines: Vec<String> = Vec::new();
    let subheader_start: usize; // index dans `header_lines`, longueur si vide.
    if recipe.header_layout.main_header_lines > 0 || recipe.header_layout.subheader_lines > 0 {
        let main_n = recipe.header_layout.main_header_lines.min(lines.len());
        header_lines.extend(lines.drain(0..main_n).map(str::to_string));
        subheader_start = header_lines.len();
        let sub_n = recipe.header_layout.subheader_lines.min(lines.len());
        header_lines.extend(lines.drain(0..sub_n).map(str::to_string));
    } else {
        match &recipe.header {
            HeaderSkip::None => {}
            HeaderSkip::Lines(n) => {
                let n = (*n).min(lines.len());
                header_lines.extend(lines.drain(0..n).map(str::to_string));
            }
            HeaderSkip::UntilMarker(marker) => {
                let marker_norm = marker.trim().to_lowercase();
                let mut found = false;
                while !lines.is_empty() {
                    let line = lines.remove(0);
                    header_lines.push(line.to_string());
                    if line.trim().to_lowercase() == marker_norm {
                        found = true;
                        break;
                    }
                }
                if !found {
                    return Err(ImportError::NoDataRows);
                }
            }
        }
        subheader_start = header_lines.len();
    }
    // Rôles des lignes de sous-en-tête (« Long Names »/« Units »/
    // « Comments »), résolus une fois pour toute la table : chaque rôle
    // pointe la **première** ligne de sa plage (voir la doc de
    // `SubheaderRoles`, une plage multi-lignes n'est pas encore répartie
    // entre colonnes différentes — limitation documentée, pas un oubli).
    let subheader_lines = &header_lines[subheader_start..];
    let role_line = |range: Option<crate::recipe::SubheaderLineRange>| {
        range.and_then(|r| subheader_lines.get(r.from.checked_sub(1)?))
    };
    let long_name_line = role_line(recipe.subheader_roles.long_name);
    let unit_line = role_line(recipe.subheader_roles.unit);
    let comment_line = role_line(recipe.subheader_roles.comment);

    // Retire le pied de page (jamais plus de lignes qu'il n'en reste).
    let footer_n = recipe.footer_skip_lines.min(lines.len());
    lines.truncate(lines.len() - footer_n);

    if lines.is_empty() {
        return Err(ImportError::NoDataRows);
    }

    // Retire le préfixe fixe de chaque ligne de données restante (utile pour
    // les préfixes de certains formats d'instrument) — jamais plus de
    // caractères qu'il n'en reste sur la ligne (une ligne trop courte devient
    // vide, pas une erreur : invariant 10). Compté en `char`, pas en octet,
    // pour ne jamais couper un caractère multi-octets en deux.
    if recipe.skip_chars > 0 {
        for line in &mut lines {
            *line = match line.char_indices().nth(recipe.skip_chars) {
                Some((byte_idx, _)) => &line[byte_idx..],
                None => "",
            };
        }
    }

    // Résout le séparateur effectif sur la première ligne restante.
    let separator = resolve_separator(recipe.separator, lines[0]);

    // Ligne d'en-tête de colonnes, le cas échéant.
    let column_names: Option<Vec<String>> = if recipe.has_column_header_row {
        let header_row = lines.remove(0);
        Some(
            separator
                .tokenize(header_row)
                .into_iter()
                .map(|t| t.trim().to_string())
                .collect(),
        )
    } else {
        None
    };

    if lines.is_empty() {
        return Err(ImportError::NoDataRows);
    }

    // Tokenise chaque ligne de sous-en-tête assignée à un rôle — une valeur
    // par colonne de données, dans le même ordre.
    let role_values = |line: Option<&String>| -> Option<Vec<String>> {
        line.map(|l| {
            separator
                .tokenize(l.as_str())
                .into_iter()
                .map(|t| t.trim().to_string())
                .collect()
        })
    };
    let long_names = role_values(long_name_line);
    let units = if recipe.subheader_roles.same_line_name_unit {
        // Même ligne que `long_name` : chaque jeton est de la forme
        // « Nom (unité) » — sans parenthèses reconnues, tout le jeton reste
        // le nom long et l'unité reste vide plutôt que d'inventer une
        // coupure arbitraire.
        long_names.as_ref().map(|names| {
            names
                .iter()
                .map(|n| match (n.find('('), n.find(')')) {
                    (Some(open), Some(close)) if open < close => {
                        n[open + 1..close].trim().to_string()
                    }
                    _ => String::new(),
                })
                .collect()
        })
    } else {
        role_values(unit_line)
    };
    let long_names = if recipe.subheader_roles.same_line_name_unit {
        long_names.map(|names| {
            names
                .into_iter()
                .map(|n| match n.find('(') {
                    Some(open) => n[..open].trim().to_string(),
                    None => n,
                })
                .collect()
        })
    } else {
        long_names
    };
    let comments = role_values(comment_line);

    // Tokenise et analyse chaque ligne de données, en gardant le jeton
    // d'origine à côté de la valeur analysée : c'est ce que `texts`
    // restitue en aval pour une cellule qui n'est pas un nombre (colonnes à
    // types mixtes).
    let rows: Vec<Vec<(f64, Option<String>)>> = lines
        .iter()
        .map(|line| {
            separator
                .tokenize(line)
                .into_iter()
                .map(|t| {
                    let value = parse_token(t, recipe.decimal);
                    let trimmed = t.trim();
                    let text = if value.is_nan() && !trimmed.is_empty() {
                        Some(trimmed.to_string())
                    } else {
                        None
                    };
                    (value, text)
                })
                .collect()
        })
        .collect();

    let num_cols = rows.iter().map(Vec::len).max().unwrap_or(0);

    let columns: Vec<ParsedColumn> = (0..num_cols)
        .map(|c| {
            let name = column_names
                .as_ref()
                .and_then(|names| names.get(c))
                .filter(|n| !n.is_empty())
                .cloned();
            let values = rows
                .iter()
                .map(|r| r.get(c).map_or(f64::NAN, |(v, _)| *v))
                .collect();
            let texts = rows
                .iter()
                .map(|r| r.get(c).and_then(|(_, t)| t.clone()))
                .collect();
            let long_name = long_names
                .as_ref()
                .and_then(|names| names.get(c))
                .filter(|n| !n.is_empty())
                .cloned();
            let unit = units
                .as_ref()
                .and_then(|values| values.get(c))
                .filter(|n| !n.is_empty())
                .cloned();
            let comment = comments
                .as_ref()
                .and_then(|values| values.get(c))
                .filter(|n| !n.is_empty())
                .cloned();
            ParsedColumn {
                name,
                values,
                texts,
                long_name,
                unit,
                comment,
            }
        })
        .collect();

    let mut warnings = Vec::new();
    if let Some(declared) = lookup_header_value(&header_lines, separator, "NPOINTS") {
        if let Ok(declared_n) = declared.trim().parse::<i64>() {
            let actual = rows.len() as i64;
            if declared_n != actual {
                warnings.push(format!(
                    "NPOINTS annonce {declared_n} lignes, {actual} lues"
                ));
            }
        }
    }

    Ok(ParsedTable {
        columns,
        warnings,
        header_lines,
        separator,
    })
}

/// Résout `Separator::Auto`/`Whitespace` en un séparateur concret, sur la
/// base de la première ligne de données restante. `pub(crate)` : réutilisé
/// par [`crate::preview`] pour que l'aperçu résolve le séparateur exactement
/// comme [`parse`].
pub(crate) fn resolve_separator(separator: Separator, first_line: &str) -> EffectiveSeparator {
    match separator {
        Separator::Comma => EffectiveSeparator::Char(','),
        Separator::Semicolon => EffectiveSeparator::Char(';'),
        Separator::Tab => EffectiveSeparator::Char('\t'),
        Separator::Whitespace => EffectiveSeparator::Whitespace,
        Separator::Auto => {
            let candidates = [
                (';', first_line.matches(';').count()),
                ('\t', first_line.matches('\t').count()),
                (',', first_line.matches(',').count()),
            ];
            match candidates.into_iter().max_by_key(|(_, count)| *count) {
                Some((c, count)) if count > 0 => EffectiveSeparator::Char(c),
                _ => EffectiveSeparator::Whitespace,
            }
        }
    }
}

/// Analyse un jeton individuel en `f64` ; `NaN` pour tout ce qui ne s'y
/// prête pas — jamais d'erreur remontée pour un seul jeton (invariant 10).
/// `pub(crate)` : réutilisé par [`crate::preview`], pour la classification
/// des cellules affichées et pour l'heuristique de détection automatique des
/// lignes d'en-tête (une ligne « ressemble à des données » si tous ses
/// jetons s'y analysent).
pub(crate) fn parse_token(token: &str, decimal: DecimalStyle) -> f64 {
    let trimmed = token.trim();
    if trimmed.is_empty() {
        return f64::NAN;
    }
    let normalized = match decimal {
        DecimalStyle::Comma => trimmed.replace(',', "."),
        DecimalStyle::Dot => trimmed.to_string(),
    };
    normalized.parse::<f64>().unwrap_or(f64::NAN)
}

/// Résout le nom de série selon `recipe.name_source` : `Filename` rend
/// `file_stem` tel quel ; `HeaderKey(key)` rend `table.header_value(key)`,
/// ou replie sur `file_stem` si la clé est absente de l'en-tête — jamais
/// d'échec dur pour un nom manquant (invariant 10).
pub fn resolve_series_name(recipe: &ImportRecipe, table: &ParsedTable, file_stem: &str) -> String {
    match &recipe.name_source {
        NameSource::Filename => file_stem.to_string(),
        NameSource::HeaderKey(key) => table
            .header_value(key)
            .unwrap_or_else(|| file_stem.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::recipe::ImportRecipe;

    #[test]
    fn fichier_binaire_rend_not_text_sans_paniquer() {
        let bytes: &[u8] = &[0xff, 0xfe, 0x00, 0x01, 0x02];
        let recipe = ImportRecipe::new("défaut");
        assert_eq!(parse(bytes, &recipe), Err(ImportError::NotText));
    }

    #[test]
    fn fichier_vide_rend_empty() {
        let recipe = ImportRecipe::new("défaut");
        assert_eq!(parse(b"", &recipe), Err(ImportError::Empty));
    }

    #[test]
    fn ligne_plus_courte_est_completee_par_nan() {
        let recipe = ImportRecipe::new("défaut");
        let table = parse(b"1,2,3\n4,5\n6,7,8\n", &recipe).unwrap();
        assert_eq!(table.columns.len(), 3);
        assert_eq!(table.row_count(), 3);
        assert_eq!(table.columns[2].values[0], 3.0);
        assert!(table.columns[2].values[1].is_nan());
        assert_eq!(table.columns[2].values[2], 8.0);
    }

    #[test]
    fn marqueur_introuvable_rend_no_data_rows() {
        let mut recipe = ImportRecipe::new("défaut");
        recipe.header = HeaderSkip::UntilMarker("XYDATA".into());
        let table = parse(b"une\ndeux\ntrois\n", &recipe);
        assert_eq!(table, Err(ImportError::NoDataRows));
    }

    #[test]
    fn resolve_series_name_replie_sur_filename_si_cle_absente() {
        let mut recipe = ImportRecipe::new("défaut");
        recipe.name_source = NameSource::HeaderKey("TITLE".into());
        let table = parse(b"1,2\n3,4\n", &recipe).unwrap();
        assert_eq!(
            resolve_series_name(&recipe, &table, "mon_fichier"),
            "mon_fichier"
        );
    }
}

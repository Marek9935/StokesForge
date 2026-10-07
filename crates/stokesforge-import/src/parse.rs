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
    /// texte à conserver (`stokesforge_core::ColumnType::Mixed`, chantier
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
    /// Valeur x' lue dans la ligne assignée au rôle `x_prime` (#164), avec
    /// le format numérique de la recette ; `None` si la ligne manque ou si le
    /// jeton n'est pas un nombre fini.
    pub x_prime: Option<f64>,
    /// Colonne lue comme dates et heures ([`crate::recipe::ColumnKind::DateTime`]) :
    /// `values` porte alors des timestamps POSIX en secondes.
    pub datetime: bool,
}

/// Le séparateur effectivement utilisé pour ce fichier, une fois
/// `Separator::Auto` (et `Separator::Whitespace`) résolu. `pub(crate)` :
/// utile aussi à [`crate::preview`] (aperçu en direct) et à la détection
/// automatique des lignes d'en-tête, pas seulement à retrouver les couples
/// clé/valeur des lignes d'en-tête ignorées.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum EffectiveSeparator {
    Char(char),
    Whitespace,
    /// Colonnes à largeur fixe : la largeur de chacune sauf la dernière.
    Fixed(Vec<usize>),
}

impl EffectiveSeparator {
    /// Découpe une ligne en jetons selon ce séparateur. Ne retire jamais les
    /// jetons vides pour un séparateur caractère (un CSV `a;;c` a bien 3
    /// champs, le second vide) ; `Whitespace` les absorbe naturellement ;
    /// `Fixed` coupe aux positions données (en caractères).
    pub(crate) fn tokenize<'a>(&self, line: &'a str) -> Vec<&'a str> {
        match self {
            EffectiveSeparator::Char(c) => line.split(*c).collect(),
            EffectiveSeparator::Whitespace => line.split_whitespace().collect(),
            EffectiveSeparator::Fixed(widths) => {
                let mut tokens = Vec::with_capacity(widths.len() + 1);
                let mut rest = line;
                for &width in widths {
                    let cut = rest
                        .char_indices()
                        .nth(width)
                        .map_or(rest.len(), |(byte, _)| byte);
                    tokens.push(&rest[..cut]);
                    rest = &rest[cut..];
                }
                tokens.push(rest);
                tokens
            }
        }
    }

    /// Coupe une ligne « CLÉ<séparateur>VALEUR » en `(clé, valeur)` à la
    /// première occurrence du séparateur. Sans objet pour des colonnes à
    /// largeur fixe : la ligne n'a alors pas de forme clé/valeur.
    pub(crate) fn split_key_value<'a>(&self, line: &'a str) -> Option<(&'a str, &'a str)> {
        match self {
            EffectiveSeparator::Char(c) => line.split_once(*c),
            EffectiveSeparator::Whitespace => line.split_once(char::is_whitespace),
            EffectiveSeparator::Fixed(_) => line.split_once(['=', ':', ';', '\t']),
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
    footer_lines: Vec<String>,
    metadata: Vec<crate::metadata::MetadataLine>,
    separator: EffectiveSeparator,
}

impl ParsedTable {
    /// Nombre de lignes de données (0 si `columns` est vide).
    pub fn row_count(&self) -> usize {
        self.columns.first().map_or(0, |c| c.values.len())
    }

    /// Métadonnées à conserver, selon la recette (en-tête, pied de page,
    /// exclusions) — voir [`crate::metadata`].
    pub fn metadata(&self) -> &[crate::metadata::MetadataLine] {
        &self.metadata
    }

    /// Lignes qui suivent la fin des données (pied de page), telles qu'elles
    /// sont dans le fichier — jamais importées.
    pub fn footer_lines(&self) -> &[String] {
        &self.footer_lines
    }

    /// Cherche, parmi les lignes d'en-tête ignorées lors du parsing, une
    /// ligne de la forme « CLÉ<séparateur>VALEUR » où CLÉ correspond à `key`
    /// (insensible à la casse, bordures blanches ignorées) — rend VALEUR
    /// (`trim`ée), ou `None` si absente. `<séparateur>` est celui
    /// effectivement résolu pour ce fichier (utile même quand la recette
    /// demandait `Separator::Auto`).
    pub fn header_value(&self, key: &str) -> Option<String> {
        lookup_header_value(&self.header_lines, &self.separator, key)
    }
}

/// Cherche `key` dans `header_lines`, sous la forme CLÉ<séparateur>VALEUR.
fn lookup_header_value(
    header_lines: &[String],
    separator: &EffectiveSeparator,
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
    #[error("the file is not readable text (binary file)")]
    NotText,
    #[error("the file is empty")]
    Empty,
    #[error("no data line found after the header")]
    NoDataRows,
}

use crate::recipe::{DecimalStyle, ImportRecipe, NameSource, Separator};

/// Parse `bytes` selon `recipe`. `Err(ImportError::NotText)` si `bytes`
/// n'est pas du texte ([`crate::decode::decode_with`] : l'encodage choisi, ou
/// détecté — UTF-8, UTF-16 avec marque, Windows-1252) — jamais de panique sur
/// un fichier binaire.
///
/// Le découpage en zones (en-tête, données, pied de page…) vient de
/// [`crate::layout::analyze`], le même que celui de l'aperçu.
///
/// Un fichier à **plusieurs blocs** (`recipe.block_split`) rend ici son premier
/// bloc ; [`parse_blocks`] les rend tous.
pub fn parse(bytes: &[u8], recipe: &ImportRecipe) -> Result<ParsedTable, ImportError> {
    parse_blocks(bytes, recipe).map(|mut tables| tables.remove(0))
}

/// Comme [`parse`], mais rend **un tableau par bloc** de données
/// (`recipe.block_split`) : un fichier qui enchaîne plusieurs spectres en donne
/// plusieurs, chacun avec son propre en-tête. Sans découpage, un seul tableau.
/// Un bloc qui n'a aucune donnée est écarté ; l'import n'échoue que si aucun
/// bloc n'en a.
pub fn parse_blocks(bytes: &[u8], recipe: &ImportRecipe) -> Result<Vec<ParsedTable>, ImportError> {
    if bytes.is_empty() {
        return Err(ImportError::Empty);
    }
    let text = crate::decode::decode_with(bytes, recipe.encoding)?;
    let mut tables = Vec::new();
    let mut last_error = ImportError::Empty;
    for block in crate::layout::split_blocks(&text, &recipe.block_split) {
        match crate::layout::analyze(block, recipe) {
            Ok(layout) => tables.push(table_from_layout(&layout, recipe)),
            Err(error) => last_error = error,
        }
    }
    if tables.is_empty() {
        Err(last_error)
    } else {
        Ok(tables)
    }
}

/// Construit la table d'un [`crate::layout::Layout`] : colonnes, sous-en-têtes,
/// métadonnées, avertissements.
pub(crate) fn table_from_layout(
    layout: &crate::layout::Layout<'_>,
    recipe: &ImportRecipe,
) -> ParsedTable {
    use crate::layout::skip_str;
    let separator = &layout.separator;
    let numbers = &layout.numbers;
    let header_lines: Vec<String> = layout
        .header
        .iter()
        .map(|&i| layout.raw[i].to_string())
        .collect();
    // Rôles des lignes de sous-en-tête (« Long Names »/« Units »/
    // « Comments »), résolus une fois : chaque rôle pointe la **première**
    // ligne de sa plage (une plage multi-lignes n'est pas répartie entre
    // colonnes — limite documentée).
    let subheader_lines = &header_lines[layout.subheader_start..];
    let role_line = |range: Option<crate::recipe::SubheaderLineRange>| {
        range.and_then(|r| subheader_lines.get(r.from.checked_sub(1)?))
    };
    let long_name_line = role_line(recipe.subheader_roles.long_name);
    let unit_line = role_line(recipe.subheader_roles.unit);
    let comment_line = role_line(recipe.subheader_roles.comment);
    let x_prime_line = role_line(recipe.subheader_roles.x_prime);

    // Ligne d'en-tête de colonnes, le cas échéant.
    let column_names: Option<Vec<String>> = layout.column_header.map(|i| {
        separator
            .tokenize(skip_str(layout.raw[i], recipe.skip_chars))
            .into_iter()
            .map(|t| t.trim().to_string())
            .collect()
    });

    // Une valeur par colonne de données, dans le même ordre, pour chaque ligne
    // de sous-en-tête assignée à un rôle.
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
        // le nom long et l'unité reste vide.
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
    let x_primes = role_values(x_prime_line);

    // Tokenise et analyse chaque ligne de données, en gardant le jeton
    // d'origine à côté de la valeur : c'est ce que `texts` restitue pour une
    // cellule qui n'est pas un nombre (colonnes à types mixtes).
    let rows: Vec<Vec<(f64, Option<String>)>> = layout
        .data
        .iter()
        .map(|&i| {
            separator
                .tokenize(skip_str(layout.raw[i], recipe.skip_chars))
                .into_iter()
                .enumerate()
                .map(|(column, t)| read_cell(t, recipe.column_kind(column), numbers))
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
            let pick = |values: &Option<Vec<String>>| {
                values
                    .as_ref()
                    .and_then(|v| v.get(c))
                    .filter(|n| !n.is_empty())
                    .cloned()
            };
            ParsedColumn {
                name,
                values,
                texts,
                long_name: pick(&long_names),
                unit: pick(&units),
                comment: pick(&comments),
                x_prime: x_primes
                    .as_ref()
                    .and_then(|v| v.get(c))
                    .map(|token| numbers.parse(token))
                    .filter(|value| value.is_finite()),
                datetime: recipe.column_kind(c) == crate::recipe::ColumnKind::DateTime,
            }
        })
        .collect();

    // Pied de page : tout ce qui suit la dernière donnée, tel quel.
    let last_data = layout.data.last().copied().unwrap_or(layout.start);
    let footer_lines: Vec<String> = layout.raw[(last_data + 1).min(layout.end)..layout.end]
        .iter()
        .map(|line| (*line).to_string())
        .collect();

    let mut warnings = Vec::new();
    if let Some(declared) = lookup_header_value(&header_lines, separator, "NPOINTS") {
        if let Ok(declared_n) = declared.trim().parse::<i64>() {
            let actual = rows.len() as i64;
            if declared_n != actual {
                warnings.push(format!(
                    "NPOINTS declares {declared_n} lines, {actual} read"
                ));
            }
        }
    }

    let metadata = crate::metadata::split_lines(
        header_lines
            .iter()
            .filter(|_| recipe.keep_header_metadata)
            .chain(footer_lines.iter().filter(|_| recipe.keep_footer_metadata))
            .map(String::as_str),
        separator,
        recipe,
    );

    ParsedTable {
        columns,
        warnings,
        header_lines,
        footer_lines,
        metadata,
        separator: separator.clone(),
    }
}

/// Lit un jeton selon la nature choisie de sa colonne : `(valeur, texte
/// conservé)`. Une seule règle, pour l'import comme pour l'aperçu (dont
/// l'annotation « texte » se déduit du second élément).
pub(crate) fn read_cell(
    token: &str,
    kind: crate::recipe::ColumnKind,
    numbers: &crate::numbers::NumberFormat,
) -> (f64, Option<String>) {
    use crate::recipe::ColumnKind;
    let trimmed = token.trim();
    match kind {
        ColumnKind::Auto => {
            let value = numbers.parse(token);
            let text = (value.is_nan() && !trimmed.is_empty() && !numbers.is_missing(token))
                .then(|| trimmed.to_string());
            (value, text)
        }
        ColumnKind::Text => (f64::NAN, (!trimmed.is_empty()).then(|| trimmed.to_string())),
        ColumnKind::DateTime => match crate::datetime::parse_datetime(trimmed) {
            Some(seconds) => (seconds, None),
            None => (
                f64::NAN,
                (!trimmed.is_empty() && !numbers.is_missing(token)).then(|| trimmed.to_string()),
            ),
        },
    }
}

/// Résout `Separator::Auto`/`Whitespace` en un séparateur concret, sur la
/// base de la première ligne restante après l'en-tête et le retrait des
/// `skip_chars` — **ligne de noms de colonnes comprise**. `pub(crate)` :
/// appelé par [`crate::layout::analyze`], seul endroit où cette décision est
/// prise pour l'import comme pour l'aperçu (issue #23).
///
/// Avec la virgule pour décimale, la virgule n'est jamais candidate : elle
/// ne peut pas être à la fois séparateur et décimale, et deux virgules
/// décimales sur une ligne battraient sinon une seule tabulation.
pub(crate) fn resolve_separator(
    separator: &Separator,
    first_line: &str,
    decimal: DecimalStyle,
) -> EffectiveSeparator {
    match separator {
        Separator::Comma => EffectiveSeparator::Char(','),
        Separator::Semicolon => EffectiveSeparator::Char(';'),
        Separator::Tab => EffectiveSeparator::Char('\t'),
        Separator::Whitespace => EffectiveSeparator::Whitespace,
        Separator::Custom(c) => EffectiveSeparator::Char(*c),
        Separator::FixedWidth(widths) => EffectiveSeparator::Fixed(widths.clone()),
        Separator::Auto => {
            let candidates = [
                (';', first_line.matches(';').count()),
                ('\t', first_line.matches('\t').count()),
                (
                    ',',
                    if decimal == DecimalStyle::Comma {
                        0
                    } else {
                        first_line.matches(',').count()
                    },
                ),
            ];
            match candidates.into_iter().max_by_key(|(_, count)| *count) {
                Some((c, count)) if count > 0 => EffectiveSeparator::Char(c),
                _ => EffectiveSeparator::Whitespace,
            }
        }
    }
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
    use crate::recipe::{HeaderSkip, ImportRecipe};

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

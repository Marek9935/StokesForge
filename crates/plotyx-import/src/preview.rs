//! Aperçu en direct du découpage d'un fichier par une [`crate::ImportRecipe`],
//! façon assistant d'import ASCII d'OriginLab : classe **chaque ligne** en
//! rôle (en-tête principal, sous-en-tête avec son rôle, en-tête de colonnes,
//! donnée, pied de page) exactement comme [`crate::parse::parse`] la
//! traiterait, pour que le panneau qui affiche cet aperçu retexte en direct à
//! chaque changement de réglage — voir
//! `docs/chantier-import-previsualisation.md` (l'objectif du chantier qui a
//! ajouté ce module).
//!
//! Point de vigilance qui a guidé toute l'écriture de ce module : rester
//! utilisable sur un fichier de plusieurs dizaines de milliers de lignes
//! sans reparser (encore moins analyser cellule à cellule) tout le fichier à
//! chaque frappe. Deux bornes distinctes y pourvoient :
//!
//! - le **découpage en lignes** (`str::split('\n')`) est fait une fois, sur
//!   tout le fichier — c'est `O(taille du fichier)`, incompressible pour
//!   savoir où sont les lignes, mais ne fait qu'indexer des tranches de la
//!   chaîne d'origine (pas de copie) ;
//! - **construire un [`PreviewLine`] possédé** (avec sa propre `String`) et
//!   **tokeniser/analyser une ligne cellule à cellule** ne sont faits que
//!   pour les lignes réellement montrées : l'en-tête et le pied de page (des
//!   comptes petits, fixés par l'utilisateur, jamais proportionnels à la
//!   taille du fichier) et au plus `recipe.preview_lines` lignes de données —
//!   jamais pour le reste, aussi long soit-il. [`FilePreview::total_data_lines`]
//!   restitue le compte réel sans avoir matérialisé chaque ligne.

use crate::parse::{parse_token, resolve_separator, ImportError};
use crate::recipe::{HeaderSkip, ImportRecipe, SubheaderRoles};

/// Rôle attribué à une ligne du fichier, pour l'aperçu. Vocabulaire calqué
/// sur la légende de l'assistant d'import ASCII d'OriginLab
/// (`S`/`L`/`U`/`C`/`MH`/`SH`, voir [`LineRole::code`]) — `P` (« System/User
/// Parameters ») n'a pas d'équivalent PlotYx, voir la doc de tête de
/// [`crate::recipe::SubheaderRoles`]. `Footer` est un rôle PlotYx en plus :
/// OriginLab ne le distingue pas dans sa légende.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineRole {
    /// Ligne blanche à une extrémité du fichier, retirée avant toute analyse
    /// (même règle que [`crate::parse::parse`]) — jamais comptée dans
    /// l'en-tête ni les données. Interne : jamais dans un [`PreviewLine`]
    /// rendu par [`preview`], seulement la valeur par défaut du tableau de
    /// travail avant classification (voir son code).
    Blank,
    /// En-tête « principal » (`HeaderLayout::main_header_lines`, ou tout
    /// l'en-tête classique `HeaderSkip::Lines`/`UntilMarker` quand la recette
    /// n'utilise pas le modèle structuré — les deux ne se distinguent pas
    /// dans ce cas, voir la doc de [`crate::recipe::HeaderLayout`]).
    MainHeader,
    /// Ligne de sous-en-tête sans rôle assigné à aucun des trois rôles
    /// reconnus.
    Subheader,
    /// Ligne de sous-en-tête assignée au rôle `long_name`.
    SubheaderLongName,
    /// Ligne de sous-en-tête assignée au rôle `unit`.
    SubheaderUnit,
    /// Ligne de sous-en-tête assignée au rôle `comment`.
    SubheaderComment,
    /// Ligne de noms courts de colonnes (`has_column_header_row`) — le `S`
    /// (« Short Name ») d'OriginLab.
    ColumnHeaderRow,
    /// Ligne de données.
    Data,
    /// Pied de page (`footer_skip_lines`).
    Footer,
}

impl LineRole {
    /// Code affiché dans l'aperçu — vocabulaire OriginLab où il existe.
    pub fn code(self) -> &'static str {
        match self {
            LineRole::Blank => "",
            LineRole::MainHeader => "MH",
            LineRole::Subheader => "SH",
            LineRole::SubheaderLongName => "L",
            LineRole::SubheaderUnit => "U",
            LineRole::SubheaderComment => "C",
            LineRole::ColumnHeaderRow => "S",
            LineRole::Data => "",
            LineRole::Footer => "F",
        }
    }
}

/// Une cellule d'une ligne de données montrée dans l'aperçu.
#[derive(Debug, Clone, PartialEq)]
pub struct PreviewCell {
    /// Jeton d'origine, `trim`é.
    pub text: String,
    /// Vrai si cette cellule deviendrait un texte conservé plutôt qu'un
    /// `NaN` muet dans une colonne mixte (jeton non vide qui ne s'analyse
    /// pas en nombre) — même règle que [`crate::parse::ParsedColumn::texts`].
    pub is_text: bool,
}

/// Une ligne du fichier telle que l'aperçu la montre.
#[derive(Debug, Clone, PartialEq)]
pub struct PreviewLine {
    /// Numéro de ligne dans le fichier d'origine, 1-based.
    pub number: usize,
    pub role: LineRole,
    pub text: String,
    /// Cellule par cellule, seulement pour une ligne de données comprise
    /// dans la fenêtre `recipe.preview_lines` — `None` pour toute autre
    /// ligne, y compris une ligne de données au-delà de cette fenêtre (voir
    /// la doc de tête du module).
    pub cells: Option<Vec<PreviewCell>>,
}

/// Le résultat d'un aperçu : les lignes à montrer, plus le compte réel de
/// lignes de données (qu'elles soient montrées ou non).
#[derive(Debug, Clone, PartialEq)]
pub struct FilePreview {
    pub lines: Vec<PreviewLine>,
    pub total_data_lines: usize,
}

/// Classe chaque ligne de `bytes` selon `recipe`, jusqu'à
/// `recipe.preview_lines` lignes de données. Mêmes conditions d'échec que
/// [`crate::parse::parse`] — `Err` exactement dans les mêmes cas, avec le
/// même sens, pour que le panneau d'aperçu montre l'erreur qu'un import
/// produirait réellement plutôt qu'une classification qui ne lui
/// correspondrait pas.
pub fn preview(bytes: &[u8], recipe: &ImportRecipe) -> Result<FilePreview, ImportError> {
    if bytes.is_empty() {
        return Err(ImportError::Empty);
    }
    let text = crate::decode::decode_text(bytes)?;
    let raw: Vec<&str> = text.split('\n').map(|l| l.trim_end_matches('\r')).collect();

    // Même règle de rognage que `parse` : lignes blanches aux deux
    // extrémités seulement.
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

    let mut roles: Vec<LineRole> = vec![LineRole::Blank; raw.len()];
    let mut idx = start;

    if recipe.header_layout.main_header_lines > 0 || recipe.header_layout.subheader_lines > 0 {
        let main_n = recipe.header_layout.main_header_lines.min(end - idx);
        for _ in 0..main_n {
            roles[idx] = LineRole::MainHeader;
            idx += 1;
        }
        let sub_n = recipe.header_layout.subheader_lines.min(end - idx);
        for s in 0..sub_n {
            roles[idx] = subheader_line_role(s + 1, &recipe.subheader_roles);
            idx += 1;
        }
    } else {
        match &recipe.header {
            HeaderSkip::None => {}
            HeaderSkip::Lines(n) => {
                let n = (*n).min(end - idx);
                for _ in 0..n {
                    roles[idx] = LineRole::MainHeader;
                    idx += 1;
                }
            }
            HeaderSkip::UntilMarker(marker) => {
                let marker_norm = marker.trim().to_lowercase();
                let mut found = false;
                while idx < end {
                    roles[idx] = LineRole::MainHeader;
                    let matched = raw[idx].trim().to_lowercase() == marker_norm;
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
    }

    let footer_n = recipe.footer_skip_lines.min(end - idx);
    let footer_start = end - footer_n;
    for role in roles.iter_mut().take(end).skip(footer_start) {
        *role = LineRole::Footer;
    }

    if idx >= footer_start {
        return Err(ImportError::NoDataRows);
    }

    // Première ligne restante après l'en-tête : c'est sur elle que `parse`
    // résout le séparateur, ligne de noms de colonnes comprise.
    let first_remaining = idx;

    if recipe.has_column_header_row {
        roles[idx] = LineRole::ColumnHeaderRow;
        idx += 1;
        if idx >= footer_start {
            return Err(ImportError::NoDataRows);
        }
    }

    let data_start = idx;

    // Le préfixe et le séparateur sont résolus comme `parse` le fait : sur
    // la première ligne restante après l'en-tête, une fois `skip_chars`
    // retiré (issue #23).
    let skip = |line: &str| -> String {
        if recipe.skip_chars == 0 {
            return line.to_string();
        }
        match line.char_indices().nth(recipe.skip_chars) {
            Some((byte_idx, _)) => line[byte_idx..].to_string(),
            None => String::new(),
        }
    };
    let separator = resolve_separator(
        recipe.separator,
        skip_str(raw[first_remaining], recipe.skip_chars),
        recipe.decimal,
    );

    // Fin des données : la même règle que `parse` (`crate::data_end`). Les
    // lignes coupées rejoignent le pied de page.
    let candidates = footer_start - data_start;
    let kept = crate::data_end::data_len(
        (data_start..footer_start).map(|i| skip_str(raw[i], recipe.skip_chars)),
        candidates,
        &recipe.data_end,
        separator,
        recipe.decimal,
    );
    if kept == 0 {
        return Err(ImportError::NoDataRows);
    }
    let footer_start = data_start + kept;
    for role in roles.iter_mut().take(end).skip(footer_start) {
        *role = LineRole::Footer;
    }
    let total_data_lines = kept;
    let shown_data_lines = total_data_lines.min(recipe.preview_lines);
    for role in roles.iter_mut().take(footer_start).skip(data_start) {
        *role = LineRole::Data;
    }

    // N'émet un `PreviewLine` possédé que pour les lignes réellement
    // montrées : en-tête/sous-en-tête/en-tête de colonnes/pied de
    // page/blancs aux extrémités (des comptes petits, fixés par
    // l'utilisateur) et au plus `preview_lines` lignes de données — jamais
    // pour le reste, voir la doc de tête du module.
    let mut lines = Vec::new();
    let mut push = |i: usize, with_cells: bool| {
        let role = roles[i];
        let cells = if with_cells {
            let skipped = skip(raw[i]);
            Some(
                separator
                    .tokenize(&skipped)
                    .into_iter()
                    .map(|t| {
                        let value = parse_token(t, recipe.decimal);
                        let trimmed = t.trim();
                        PreviewCell {
                            text: trimmed.to_string(),
                            is_text: value.is_nan() && !trimmed.is_empty(),
                        }
                    })
                    .collect(),
            )
        } else {
            None
        };
        lines.push(PreviewLine {
            number: i + 1,
            role,
            text: raw[i].to_string(),
            cells,
        });
    };
    // Les lignes blanches aux deux extrémités (hors `[start, end)`) ne sont
    // pas montrées : `parse` les retire exactement de la même façon, ce
    // n'est donc pas une perte d'information nouvelle par rapport à
    // aujourd'hui — seulement pas un gain, documenté comme tel dans
    // `docs/chantier-import-previsualisation.md`.
    for i in start..data_start {
        push(i, false);
    }
    for i in data_start..(data_start + shown_data_lines) {
        push(i, true);
    }
    for i in footer_start..end {
        push(i, false);
    }

    Ok(FilePreview {
        lines,
        total_data_lines,
    })
}

/// `line` privée de ses `skip_chars` premiers caractères (comme `parse`).
fn skip_str(line: &str, skip_chars: usize) -> &str {
    if skip_chars == 0 {
        return line;
    }
    match line.char_indices().nth(skip_chars) {
        Some((byte_idx, _)) => &line[byte_idx..],
        None => "",
    }
}

/// Rôle d'une ligne de sous-en-tête, 1-based **dans le bloc de
/// sous-en-tête** (indépendant du nombre de lignes d'en-tête principal qui
/// le précèdent) — une ligne qui tombe dans plusieurs plages assignées (cas
/// improbable mais possible : rien n'empêche l'utilisateur de faire se
/// chevaucher deux rôles) suit la priorité `long_name` > `unit` > `comment`,
/// documentée ici plutôt que de porter plusieurs rôles à la fois — ce que
/// [`LineRole`] ne modélise pas.
fn subheader_line_role(sub_index: usize, roles: &SubheaderRoles) -> LineRole {
    let contains = |range: Option<crate::recipe::SubheaderLineRange>| {
        range.is_some_and(|r| r.from <= sub_index && sub_index <= r.to)
    };
    if contains(roles.long_name) {
        LineRole::SubheaderLongName
    } else if !roles.same_line_name_unit && contains(roles.unit) {
        LineRole::SubheaderUnit
    } else if contains(roles.comment) {
        LineRole::SubheaderComment
    } else {
        LineRole::Subheader
    }
}

/// Détecte automatiquement `(main_header_lines, subheader_lines)` à partir
/// du contenu du fichier — le bouton de réévaluation d'OriginLab
/// (`auto_detect`, voir [`crate::recipe::HeaderLayout`]).
///
/// Heuristique en deux passes, sur les 500 premières lignes non blanches du
/// fichier au plus (un plafond documenté, pas une erreur : un en-tête plus
/// long que ça n'existe pas en pratique dans ce domaine, et ça borne le coût
/// indépendamment de la taille du fichier — même esprit que les six plafonds
/// de `docs/archive/rustrewrite/questions-ouvertes.md` §11) :
///
/// 1. **En-tête principal** : à partir du début, tant qu'une ligne n'a pas le
///    même nombre de champs (une fois tokenisée par `separator`) que la
///    ligne de référence (la dernière ligne examinée, généralement une
///    ligne de données) ;
/// 2. **Sous-en-tête** : à partir de là, tant qu'une ligne a le bon nombre de
///    champs mais qu'au moins un de ses jetons ne s'analyse pas en nombre
///    (une ligne de sous-en-tête « aligne » ses champs sur les colonnes sans
///    en être elle-même une donnée numérique — un nom de colonne ou une
///    unité, typiquement).
///
/// Limite connue, documentée plutôt que corrigée : un fichier sans aucune
/// ligne de données parmi les 500 examinées (en-tête anormalement long, ou
/// fichier qui n'a que du texte) fait dégénérer la détection à « tout est
/// en-tête principal, rien n'est sous-en-tête » — pas un panic, juste une
/// détection peu utile, à corriger à la main comme n'importe quelle
/// détection automatique qui se trompe.
pub fn detect_header_layout(
    bytes: &[u8],
    separator: crate::recipe::Separator,
    decimal: crate::recipe::DecimalStyle,
) -> crate::recipe::HeaderLayout {
    const SCAN_CAP: usize = 500;
    let Ok(text) = crate::decode::decode_text(bytes) else {
        return crate::recipe::HeaderLayout::default();
    };
    let lines: Vec<&str> = text
        .split('\n')
        .map(|l| l.trim_end_matches('\r'))
        .filter(|l| !l.trim().is_empty())
        .take(SCAN_CAP)
        .collect();
    if lines.is_empty() {
        return crate::recipe::HeaderLayout::default();
    }
    let effective = resolve_separator(separator, lines[lines.len() - 1], decimal);
    let ref_count = effective.tokenize(lines[lines.len() - 1]).len();

    let mut i = 0usize;
    let mut main = 0usize;
    while i < lines.len() && effective.tokenize(lines[i]).len() != ref_count {
        main += 1;
        i += 1;
    }
    let mut sub = 0usize;
    while i < lines.len() {
        let tokens = effective.tokenize(lines[i]);
        let all_numeric = tokens
            .iter()
            .all(|t| t.trim().is_empty() || !parse_token(t, decimal).is_nan());
        if all_numeric {
            break;
        }
        sub += 1;
        i += 1;
    }
    crate::recipe::HeaderLayout {
        main_header_lines: main,
        subheader_lines: sub,
        auto_detect: true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::recipe::{
        HeaderLayout, ImportRecipe, Separator, SubheaderLineRange, SubheaderRoles,
    };

    #[test]
    fn sans_reglage_structure_tout_est_donnee() {
        let recipe = ImportRecipe::new("défaut");
        let result = preview(b"1,2,3\n4,5,6\n", &recipe).unwrap();
        assert_eq!(result.total_data_lines, 2);
        assert!(result.lines.iter().all(|l| l.role == LineRole::Data));
        assert_eq!(result.lines[0].cells.as_ref().unwrap().len(), 3);
    }

    #[test]
    fn en_tete_structure_classe_principal_et_sous_en_tete() {
        let mut recipe = ImportRecipe::new("défaut");
        recipe.header_layout = HeaderLayout {
            main_header_lines: 1,
            subheader_lines: 2,
            auto_detect: false,
        };
        recipe.subheader_roles = SubheaderRoles {
            long_name: Some(SubheaderLineRange::single(1)),
            unit: Some(SubheaderLineRange::single(2)),
            comment: None,
            same_line_name_unit: false,
        };
        let bytes = b"Instrument XYZ\nTemperature,Pressure\ndegC,bar\n1,2\n3,4\n";
        let result = preview(bytes, &recipe).unwrap();
        let roles: Vec<LineRole> = result.lines.iter().map(|l| l.role).collect();
        assert_eq!(
            roles,
            vec![
                LineRole::MainHeader,
                LineRole::SubheaderLongName,
                LineRole::SubheaderUnit,
                LineRole::Data,
                LineRole::Data,
            ]
        );
        assert_eq!(result.total_data_lines, 2);
    }

    #[test]
    fn cellule_non_numerique_est_annotee_texte() {
        let recipe = ImportRecipe::new("défaut");
        let result = preview(b"1,n/a,3\n", &recipe).unwrap();
        let cells = result.lines[0].cells.as_ref().unwrap();
        assert!(!cells[0].is_text);
        assert!(cells[1].is_text);
        assert_eq!(cells[1].text, "n/a");
        assert!(!cells[2].is_text);
    }

    #[test]
    fn preview_lines_borne_les_cellules_analysees() {
        let mut recipe = ImportRecipe::new("défaut");
        recipe.preview_lines = 2;
        let bytes = b"1\n2\n3\n4\n5\n";
        let result = preview(bytes, &recipe).unwrap();
        assert_eq!(result.total_data_lines, 5);
        let with_cells = result.lines.iter().filter(|l| l.cells.is_some()).count();
        assert_eq!(with_cells, 2);
        // Les lignes au-delà de la fenêtre ne sont même pas émises : coût
        // borné indépendamment du nombre réel de lignes de données.
        assert_eq!(result.lines.len(), 2);
    }

    #[test]
    fn marqueur_introuvable_rend_la_meme_erreur_que_parse() {
        let mut recipe = ImportRecipe::new("défaut");
        recipe.header = crate::recipe::HeaderSkip::UntilMarker("XYDATA".into());
        let result = preview(b"une\ndeux\ntrois\n", &recipe);
        assert_eq!(result, Err(ImportError::NoDataRows));
    }

    #[test]
    fn detecte_en_tete_et_sous_en_tete_sur_un_fichier_plausible() {
        let bytes =
            b"Instrument XYZ\nDate: 2024-01-01\nTemperature,Pressure\ndegC,bar\n1,2\n3,4\n5,6\n";
        let layout =
            detect_header_layout(bytes, Separator::Comma, crate::recipe::DecimalStyle::Dot);
        assert_eq!(layout.main_header_lines, 2);
        assert_eq!(layout.subheader_lines, 2);
        assert!(layout.auto_detect);
    }

    #[test]
    fn detection_sur_fichier_binaire_ne_panique_pas() {
        let layout = detect_header_layout(
            &[0xff, 0xfe, 0x00],
            Separator::Auto,
            crate::recipe::DecimalStyle::Dot,
        );
        assert_eq!(layout, crate::recipe::HeaderLayout::default());
    }
}

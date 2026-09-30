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

use crate::numbers::NumberFormat;
use crate::parse::{resolve_separator, ImportError};
use crate::recipe::{ImportRecipe, SubheaderRoles};

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
    /// Pied de page : `footer_skip_lines`, et tout ce qui suit la fin des
    /// données ([`crate::recipe::DataEnd`]).
    Footer,
    /// Ligne de commentaire (`comment_prefix`) : jamais une donnée.
    Comment,
    /// Ligne de données ignorée au début (`skip_data_rows`).
    Skipped,
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
            LineRole::Comment => "#",
            LineRole::Skipped => "x",
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
    /// Nombre de blocs de données du fichier (`recipe.block_split`) ; l'aperçu
    /// montre le premier.
    pub block_count: usize,
}

/// Nombre maximal de lignes autres que des données (en-tête, pied de page,
/// commentaires, lignes ignorées) montrées par l'aperçu : un pied de page de
/// plusieurs millions de lignes ne doit pas devenir autant d'allocations.
const MAX_OTHER_LINES: usize = 500;

/// Classe chaque ligne de `bytes` selon `recipe`, jusqu'à
/// `recipe.preview_lines` lignes de données. Mêmes conditions d'échec que
/// [`crate::parse::parse`] — `Err` exactement dans les mêmes cas, avec le
/// même sens : les deux s'appuient sur le **même** découpage
/// ([`crate::layout::analyze`]), donc l'aperçu montre ce que l'import fera.
pub fn preview(bytes: &[u8], recipe: &ImportRecipe) -> Result<FilePreview, ImportError> {
    if bytes.is_empty() {
        return Err(ImportError::Empty);
    }
    let text = crate::decode::decode_with(bytes, recipe.encoding)?;
    let blocks = crate::layout::split_blocks(&text, &recipe.block_split);
    let block_count = blocks.len().max(1);
    let first_block = blocks.first().copied().unwrap_or(&text);
    let layout = crate::layout::analyze(first_block, recipe)?;
    let total_data_lines = layout.data.len();
    let shown_data_lines = total_data_lines.min(recipe.preview_lines);

    // N'émet un `PreviewLine` possédé que pour les lignes réellement
    // montrées : en-tête, pied de page et autres zones (des comptes
    // bornés par `MAX_OTHER_LINES`) et au plus `preview_lines` lignes de
    // données — jamais pour le reste, voir la doc de tête du module.
    let mut lines = Vec::new();
    let (mut data_seen, mut others) = (0usize, 0usize);
    for i in layout.start..layout.end {
        let role = layout.roles[i];
        let with_cells = role == LineRole::Data;
        if with_cells {
            if data_seen >= shown_data_lines {
                continue;
            }
            data_seen += 1;
        } else {
            if others >= MAX_OTHER_LINES {
                continue;
            }
            others += 1;
        }
        let cells = with_cells.then(|| {
            layout
                .separator
                .tokenize(crate::layout::skip_str(layout.raw[i], recipe.skip_chars))
                .into_iter()
                .enumerate()
                .map(|(column, t)| {
                    let (_, text) =
                        crate::parse::read_cell(t, recipe.column_kind(column), &layout.numbers);
                    PreviewCell {
                        text: t.trim().to_string(),
                        is_text: text.is_some(),
                    }
                })
                .collect()
        });
        lines.push(PreviewLine {
            number: i + 1,
            role,
            text: layout.raw[i].to_string(),
            cells,
        });
    }

    Ok(FilePreview {
        lines,
        total_data_lines,
        block_count,
    })
}

/// Rôle d'une ligne de sous-en-tête, 1-based **dans le bloc de
/// sous-en-tête** (indépendant du nombre de lignes d'en-tête principal qui
/// le précèdent) — une ligne qui tombe dans plusieurs plages assignées (cas
/// improbable mais possible : rien n'empêche l'utilisateur de faire se
/// chevaucher deux rôles) suit la priorité `long_name` > `unit` > `comment`,
/// documentée ici plutôt que de porter plusieurs rôles à la fois — ce que
/// [`LineRole`] ne modélise pas.
pub(crate) fn subheader_line_role(sub_index: usize, roles: &SubheaderRoles) -> LineRole {
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
    let numbers = NumberFormat::new(decimal, None, &[], lines.iter().copied());
    let effective = resolve_separator(&separator, lines[lines.len() - 1], numbers.decimal);
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
            .all(|t| t.trim().is_empty() || !numbers.parse(t).is_nan());
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

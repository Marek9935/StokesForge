//! Grammaire des chemins d'objets Origin : `[Book]Sheet!Col`, avec dossiers
//! optionnels en préfixe (`/Manips/[Book1]Sheet1!ColA`).

use crate::error::{CoreError, CoreResult};
use crate::name::RESERVED_NAME_CHARS;

/// Niveau le plus fin désigné par un chemin.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PathDepth {
    /// Le chemin désigne un dossier (ou la racine).
    Folder,
    /// Le chemin désigne une fenêtre (classeur, graphe, matrice, note).
    Window,
    /// Le chemin désigne une feuille à l'intérieur d'un classeur.
    Sheet,
    /// Le chemin désigne une colonne à l'intérieur d'une feuille.
    Column,
}

/// Chemin analysé. Les champs valent `None` quand ils sont absents.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ObjectPath {
    /// Segments de dossier, de la racine (ou du point de départ relatif) vers la feuille.
    pub folders: Vec<String>,
    /// Nom de la fenêtre (classeur, graphe, matrice, note), si présente.
    pub window: Option<String>,
    /// Nom de la feuille, si présente.
    pub sheet: Option<String>,
    /// Nom de la colonne, si présente.
    pub column: Option<String>,
    /// `true` si le chemin est ancré à la racine (préfixé par `/`).
    pub rooted: bool,
}

impl ObjectPath {
    /// Niveau le plus fin désigné par ce chemin.
    pub fn depth(&self) -> PathDepth {
        if self.column.is_some() {
            PathDepth::Column
        } else if self.sheet.is_some() {
            PathDepth::Sheet
        } else if self.window.is_some() {
            PathDepth::Window
        } else {
            PathDepth::Folder
        }
    }
}

impl std::fmt::Display for ObjectPath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", format_path(self))
    }
}

impl std::str::FromStr for ObjectPath {
    type Err = CoreError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        parse_path(s)
    }
}

/// Valide un segment de chemin (dossier, fenêtre, feuille ou colonne) : non
/// vide, sans espace de bordure, sans caractère réservé.
fn check_segment(segment: &str) -> CoreResult<&str> {
    if segment.is_empty() || segment != segment.trim() {
        return Err(CoreError::Path(format!("segment invalide : {segment}")));
    }
    if segment.contains(RESERVED_NAME_CHARS) {
        return Err(CoreError::Path(format!("segment invalide : {segment}")));
    }
    Ok(segment)
}

/// Découpe un préfixe de dossiers (`Manips/Jour1/`) en segments validés,
/// en ignorant les segments vides (`//` répétés).
fn parse_folder_segments(prefix: &str) -> CoreResult<Vec<String>> {
    prefix
        .split('/')
        .filter(|segment| !segment.is_empty())
        .map(|segment| check_segment(segment).map(str::to_string))
        .collect()
}

/// Analyse un chemin. `Err(CoreError::Path(_))` si la syntaxe est invalide.
pub fn parse_path(text: &str) -> CoreResult<ObjectPath> {
    let raw = text.trim();
    if raw.is_empty() {
        return Err(CoreError::Path("chemin vide".to_string()));
    }

    if !raw.contains('[') {
        if raw.contains(']') || raw.contains('!') {
            return Err(CoreError::Path("chemin mal formé".to_string()));
        }
        let rooted = raw.starts_with('/');
        let folders = parse_folder_segments(raw)?;
        if folders.is_empty() && !rooted {
            return Err(CoreError::Path("chemin mal formé".to_string()));
        }
        return Ok(ObjectPath {
            folders,
            rooted,
            ..Default::default()
        });
    }

    let Some((prefix, rest)) = raw.split_once('[') else {
        return Err(CoreError::Path("chemin mal formé".to_string()));
    };

    if !prefix.is_empty() && !prefix.ends_with('/') {
        return Err(CoreError::Path(
            "préfixe de dossier non terminé par '/'".to_string(),
        ));
    }
    let rooted = prefix.starts_with('/');
    let folders = parse_folder_segments(prefix)?;

    let Some((window, tail)) = rest.split_once(']') else {
        return Err(CoreError::Path("crochet fermant manquant".to_string()));
    };
    let window = check_segment(window)?.to_string();

    if tail.contains('[') || tail.contains(']') {
        return Err(CoreError::Path("crochet en trop".to_string()));
    }

    let (sheet_part, bang_present, column_part) = match tail.split_once('!') {
        Some((sheet_part, column_part)) => (sheet_part, true, column_part),
        None => (tail, false, ""),
    };

    let sheet = if sheet_part.is_empty() {
        None
    } else {
        Some(check_segment(sheet_part)?.to_string())
    };

    let column = if bang_present {
        let column = check_segment(column_part)?.to_string();
        if sheet.is_none() {
            return Err(CoreError::Path("colonne sans feuille".to_string()));
        }
        Some(column)
    } else {
        None
    };

    Ok(ObjectPath {
        folders,
        window: Some(window),
        sheet,
        column,
        rooted,
    })
}

/// Forme textuelle canonique.
pub fn format_path(path: &ObjectPath) -> String {
    let mut prefix = path.folders.join("/");
    if path.rooted {
        prefix = format!("/{prefix}");
    }

    let Some(window) = &path.window else {
        return if prefix.is_empty() {
            "/".to_string()
        } else {
            prefix
        };
    };

    if !prefix.is_empty() && !prefix.ends_with('/') {
        prefix.push('/');
    }
    let mut out = format!("{prefix}[{window}]");
    if let Some(sheet) = &path.sheet {
        out.push_str(sheet);
    }
    if let Some(column) = &path.column {
        out.push('!');
        out.push_str(column);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn parses_full_column_path() {
        let path = parse_path("[Book1]Sheet1!ColA").expect("chemin valide");
        assert!(path.folders.is_empty());
        assert_eq!(path.window.as_deref(), Some("Book1"));
        assert_eq!(path.sheet.as_deref(), Some("Sheet1"));
        assert_eq!(path.column.as_deref(), Some("ColA"));
        assert_eq!(path.depth(), PathDepth::Column);
        assert!(!path.rooted);
    }

    #[test]
    fn parses_sheet_path() {
        let path = parse_path("[Book1]Sheet1").expect("chemin valide");
        assert_eq!(path.depth(), PathDepth::Sheet);
        assert_eq!(path.column, None);
    }

    #[test]
    fn parses_window_only_path() {
        let path = parse_path("[Graph1]").expect("chemin valide");
        assert_eq!(path.depth(), PathDepth::Window);
    }

    #[test]
    fn parses_root() {
        let path = parse_path("/").expect("chemin valide");
        assert!(path.folders.is_empty());
        assert!(path.rooted);
        assert_eq!(path.depth(), PathDepth::Folder);
    }

    #[test]
    fn parses_rooted_folder_path() {
        let path = parse_path("/Manips/Jour1").expect("chemin valide");
        assert_eq!(
            path.folders,
            vec!["Manips".to_string(), "Jour1".to_string()]
        );
        assert!(path.rooted);
    }

    #[test]
    fn parses_relative_folder_path() {
        let path = parse_path("Manips/Jour1").expect("chemin valide");
        assert_eq!(
            path.folders,
            vec!["Manips".to_string(), "Jour1".to_string()]
        );
        assert!(!path.rooted);
    }

    #[test]
    fn ignores_empty_segments_between_double_slashes() {
        let path = parse_path("/Manips//Jour1").expect("chemin valide");
        assert_eq!(
            path.folders,
            vec!["Manips".to_string(), "Jour1".to_string()]
        );
    }

    #[test]
    fn parses_folder_prefixed_column_path() {
        let path = parse_path("/Manips/Jour1/[Book1]Sheet1!ColA").expect("chemin valide");
        assert_eq!(
            path.folders,
            vec!["Manips".to_string(), "Jour1".to_string()]
        );
        assert_eq!(path.window.as_deref(), Some("Book1"));
        assert!(path.rooted);
    }

    #[test]
    fn tolerates_surrounding_whitespace() {
        let path = parse_path("  [Book1]Sheet1  ").expect("chemin valide");
        assert_eq!(path.sheet.as_deref(), Some("Sheet1"));
    }

    #[test]
    fn rejects_malformed_paths() {
        let cases = [
            "",
            "   ",
            "[Book1",
            "Book1]Sheet1",
            "[]Sheet1",
            "[Book1]!ColA",
            "Manips[Book1]Sheet1",
            "[Book1]Sheet1!",
            "[Book1]Sheet[2]",
            "a!b",
        ];
        for case in cases {
            assert!(
                matches!(parse_path(case), Err(CoreError::Path(_))),
                "attendu CoreError::Path pour {case:?}"
            );
        }
    }

    #[test]
    fn round_trips_through_format_path() {
        let cases = [
            "/",
            "/Manips",
            "/Manips/Jour1",
            "[Book1]",
            "[Book1]Sheet1",
            "[Book1]Sheet1!ColA",
            "/Manips/[Book1]Sheet1!ColA",
        ];
        for case in cases {
            let parsed = parse_path(case).expect("chemin valide");
            assert_eq!(format_path(&parsed), case, "round-trip pour {case:?}");
            assert_eq!(parsed.to_string(), case, "Display pour {case:?}");
        }
    }

    #[test]
    fn from_str_matches_parse_path() {
        let case = "[Book1]Sheet1!ColA";
        assert_eq!(ObjectPath::from_str(case), parse_path(case));
        assert_eq!(case.parse::<ObjectPath>(), parse_path(case));
    }
}

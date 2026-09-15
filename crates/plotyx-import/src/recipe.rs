//! Le modèle de recette : la description figée (sérialisable) de comment un
//! appareil donné écrit ses fichiers, sans rien connaître du moteur de
//! parsing lui-même (voir [`crate::parse`]).

/// Une recette de parsing réutilisable pour un format de fichier d'appareil.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ImportRecipe {
    pub name: String,
    pub separator: Separator,
    pub decimal: DecimalStyle,
    pub header: HeaderSkip,
    pub footer_skip_lines: usize,
    pub has_column_header_row: bool,
    pub name_source: NameSource,
}

/// Séparateur de colonnes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Separator {
    Comma,
    Semicolon,
    Tab,
    Whitespace,
    /// Détecté automatiquement à partir de la première ligne de données.
    Auto,
}

/// Convention de séparateur décimal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum DecimalStyle {
    Dot,
    Comma,
}

/// Combien de lignes ignorer avant les données.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum HeaderSkip {
    None,
    /// Ignore les `n` premières lignes, quel que soit leur contenu.
    Lines(usize),
    /// Ignore tout jusqu'à la première ligne dont le texte, une fois
    /// bordures blanches retirées, est égal (comparaison insensible à la
    /// casse) à ce marqueur — cette ligne elle-même est ignorée aussi.
    UntilMarker(String),
}

/// D'où vient le nom de la série importée.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum NameSource {
    /// Le nom de fichier (sans extension), fourni par l'appelant.
    Filename,
    /// Une valeur lue dans les lignes d'en-tête ignorées : cherche une ligne
    /// « CLÉ<séparateur>VALEUR » où CLÉ correspond à cette chaîne
    /// (insensible à la casse, bordures blanches ignorées des deux côtés),
    /// et prend VALEUR (`trim`ée) comme nom.
    HeaderKey(String),
}

impl ImportRecipe {
    /// Nouvelle recette aux réglages les plus permissifs.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            separator: Separator::Auto,
            decimal: DecimalStyle::Dot,
            header: HeaderSkip::None,
            footer_skip_lines: 0,
            has_column_header_row: false,
            name_source: NameSource::Filename,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_est_permissive_par_defaut() {
        let recipe = ImportRecipe::new("test");
        assert_eq!(recipe.name, "test");
        assert_eq!(recipe.separator, Separator::Auto);
        assert_eq!(recipe.decimal, DecimalStyle::Dot);
        assert_eq!(recipe.header, HeaderSkip::None);
        assert_eq!(recipe.footer_skip_lines, 0);
        assert!(!recipe.has_column_header_row);
        assert_eq!(recipe.name_source, NameSource::Filename);
    }

    #[test]
    fn round_trip_json() {
        let recipe = ImportRecipe {
            name: "Jasco".into(),
            separator: Separator::Semicolon,
            decimal: DecimalStyle::Comma,
            header: HeaderSkip::UntilMarker("XYDATA".into()),
            footer_skip_lines: 2,
            has_column_header_row: false,
            name_source: NameSource::HeaderKey("TITLE".into()),
        };
        let json = serde_json::to_string(&recipe).unwrap();
        let back: ImportRecipe = serde_json::from_str(&json).unwrap();
        assert_eq!(recipe, back);
    }
}
